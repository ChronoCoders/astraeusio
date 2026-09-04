//! Drives the observed_at comparison through the DuckDB build this crate links,
//! rather than through the Python client every earlier result came from.
//!
//! Builds each reachable state on a copy of a source database and, for every
//! state, compares a range predicate the scan can prune against the same range
//! written so it cannot. Wrapping the scan in `OFFSET 0` blocks filter pushdown,
//! so the second form cannot prune and stands in for the truth.
//!
//! Usage: cargo run --example observed_at_probe -- <source db> [artifact ...]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use duckdb::Connection;

const TABLES: [&str; 6] = ["kp", "kp_3h", "solar_wind", "xray", "imf", "dst"];
const OBS: &str = "epoch(time_tag::TIMESTAMP)::BIGINT";
const SENT_MIN: i64 = i64::MAX;
const SENT_MAX: i64 = i64::MIN;

struct Counts {
    sentinel: usize,
    merged: usize,
    narrow: usize,
}

fn copy_db(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    let wal_dest = PathBuf::from(format!("{}.wal", dest.display()));
    if wal_dest.exists() {
        std::fs::remove_file(&wal_dest)?;
    }
    std::fs::copy(src, dest).with_context(|| format!("copying {}", src.display()))?;
    let wal_src = PathBuf::from(format!("{}.wal", src.display()));
    if wal_src.exists() {
        std::fs::copy(&wal_src, &wal_dest)?;
    }
    // Preserved artifacts are kept read only, and the copy inherits that, which
    // DuckDB refuses to open. The copy is ours to write to.
    clear_readonly(dest)?;
    if wal_dest.exists() {
        clear_readonly(&wal_dest)?;
    }
    Ok(())
}

fn clear_readonly(path: &Path) -> Result<()> {
    let mut perms = std::fs::metadata(path)?.permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

fn parse_min_max(stats: &str) -> Option<(i64, i64)> {
    let rest = stats.strip_prefix("[Min: ")?;
    let (min_s, rest) = rest.split_once(", Max: ")?;
    let (max_s, _) = rest.split_once(']')?;
    Some((min_s.parse().ok()?, max_s.parse().ok()?))
}

/// Classifies every row group by whether its recorded bounds describe its rows.
fn classify(conn: &Connection) -> Result<Counts> {
    let mut c = Counts {
        sentinel: 0,
        merged: 0,
        narrow: 0,
    };
    for table in TABLES {
        let mut stmt = match conn.prepare(
            "SELECT count, stats FROM pragma_storage_info(?) \
             WHERE column_name = 'observed_at' AND segment_type <> 'VALIDITY' \
             ORDER BY row_group_id",
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let rows: Vec<(i64, String)> = match stmt.query_map([table], |r| Ok((r.get(0)?, r.get(1)?)))
        {
            Ok(it) => it.collect::<Result<_, _>>()?,
            Err(_) => continue,
        };
        let mut offset = 0i64;
        for (count, stats) in rows {
            let start = offset;
            offset += count;
            let Some((rmin, rmax)) = parse_min_max(&stats) else {
                continue;
            };
            if rmin == SENT_MIN && rmax == SENT_MAX {
                c.sentinel += 1;
                continue;
            }
            let bounds: (Option<i64>, Option<i64>) = conn.query_row(
                &format!(
                    "SELECT min(observed_at), max(observed_at) FROM {table} \
                     WHERE rowid >= ? AND rowid < ?"
                ),
                duckdb::params![start, start + count],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            match bounds {
                (Some(amin), Some(amax)) if rmax < amax || rmin > amin => c.narrow += 1,
                (Some(_), Some(_)) => c.merged += 1,
                // An empty rowid range means the offset walk and the storage
                // layout disagree. Counting it as merged would hide that.
                _ => println!(
                    "    WARNING empty rowid range for {table} start={start} count={count}"
                ),
            }
        }
    }
    Ok(c)
}

/// Count form, plus the three shapes the endpoints actually run.
fn compare(conn: &Connection, label: &str) -> Result<(usize, usize)> {
    let mut total = 0usize;
    let mut wrong = 0usize;
    for table in TABLES {
        let bounds: (Option<i64>, Option<i64>) = match conn.query_row(
            &format!("SELECT min(observed_at), max(observed_at) FROM {table}"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let (Some(lo), Some(hi)) = bounds else {
            continue;
        };

        for i in 0..=20 {
            let probe = lo + (hi - lo) * i / 20;

            let pruned: i64 = conn.query_row(
                &format!("SELECT count(*) FROM {table} WHERE observed_at > ?"),
                duckdb::params![probe],
                |r| r.get(0),
            )?;
            let truth: i64 = conn.query_row(
                &format!(
                    "SELECT count(*) FROM (SELECT observed_at FROM {table} OFFSET 0) \
                     WHERE observed_at > ?"
                ),
                duckdb::params![probe],
                |r| r.get(0),
            )?;
            total += 1;
            if pruned != truth {
                wrong += 1;
                println!(
                    "    DISAGREE count {label} {table} probe={probe} pruned={pruned} truth={truth}"
                );
            }

            if table == "kp" {
                let group_sql = "SELECT MIN(time_tag), CAST(AVG(estimated_kp_e2) AS BIGINT) \
                                 FROM {SRC} WHERE observed_at > ? \
                                 GROUP BY observed_at / 900 ORDER BY 1";
                let a = fetch_pairs(conn, &group_sql.replace("{SRC}", "kp"), probe)?;
                let b = fetch_pairs(
                    conn,
                    &group_sql.replace("{SRC}", "(SELECT * FROM kp OFFSET 0)"),
                    probe,
                )?;
                total += 1;
                if a != b {
                    wrong += 1;
                    println!(
                        "    DISAGREE group_by {label} probe={probe} pruned_rows={} truth_rows={}",
                        a.len(),
                        b.len()
                    );
                }

                let agg_sql = "SELECT AVG(estimated_kp_e2), MAX(estimated_kp_e2), COUNT(*) \
                               FROM {SRC} WHERE observed_at > ?";
                let a = fetch_agg(conn, &agg_sql.replace("{SRC}", "kp"), probe)?;
                let b = fetch_agg(
                    conn,
                    &agg_sql.replace("{SRC}", "(SELECT * FROM kp OFFSET 0)"),
                    probe,
                )?;
                total += 1;
                if a != b {
                    wrong += 1;
                    println!("    DISAGREE aggregates {label} probe={probe} {a:?} vs {b:?}");
                }
            }
        }

        if table == "kp" {
            let order_sql = "SELECT time_tag FROM {SRC} ORDER BY observed_at DESC LIMIT 1440";
            let a = fetch_strings(conn, &order_sql.replace("{SRC}", "kp"))?;
            let b = fetch_strings(
                conn,
                &order_sql.replace("{SRC}", "(SELECT * FROM kp OFFSET 0)"),
            )?;
            total += 1;
            if a != b {
                wrong += 1;
                println!(
                    "    DISAGREE order_by_limit {label} pruned_rows={} truth_rows={}",
                    a.len(),
                    b.len()
                );
            }
        }
    }
    Ok((total, wrong))
}

fn fetch_pairs(conn: &Connection, sql: &str, probe: i64) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(duckdb::params![probe], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn fetch_agg(conn: &Connection, sql: &str, probe: i64) -> Result<(Option<f64>, Option<i64>, i64)> {
    Ok(conn.query_row(sql, duckdb::params![probe], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })?)
}

fn fetch_strings(conn: &Connection, sql: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn add_column(conn: &Connection) -> Result<()> {
    for table in TABLES {
        let _ = conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN observed_at BIGINT"
        ));
    }
    Ok(())
}

fn backfill(conn: &Connection) -> Result<()> {
    for table in TABLES {
        conn.execute_batch(&format!(
            "UPDATE {table} SET observed_at = {OBS} WHERE observed_at IS NULL"
        ))?;
    }
    Ok(())
}

fn run_state(src: &Path, name: &str, build: impl Fn(&Connection) -> Result<()>) -> Result<usize> {
    let dest = PathBuf::from(format!("probe_{name}.duckdb"));
    copy_db(src, &dest)?;
    let conn = Connection::open(&dest)?;
    build(&conn)?;
    let c = classify(&conn)?;
    let (total, wrong) = compare(&conn, name)?;
    println!(
        "  [{name:<22}] sentinel={} merged={} narrow={} comparisons={total} disagreements={wrong}",
        c.sentinel, c.merged, c.narrow
    );
    drop(conn);
    let _ = std::fs::remove_file(&dest);
    let _ = std::fs::remove_file(format!("{}.wal", dest.display()));
    Ok(wrong)
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let src = args
        .next()
        .ok_or_else(|| anyhow!("usage: observed_at_probe <source db> [artifact ...]"))?;
    let src = PathBuf::from(src);

    let version: String =
        Connection::open_in_memory()?.query_row("SELECT version()", [], |r| r.get(0))?;
    println!("duckdb version reported by this binary: {version}");
    println!();

    let mut wrong = 0usize;

    wrong += run_state(&src, "sentinel", |c| {
        add_column(c)?;
        backfill(c)
    })?;
    wrong += run_state(&src, "merged", |c| {
        add_column(c)?;
        backfill(c)?;
        c.execute_batch("CHECKPOINT")?;
        Ok(())
    })?;
    wrong += run_state(&src, "mixed", |c| {
        add_column(c)?;
        for table in TABLES {
            let first: Option<i64> = c
                .query_row(
                    "SELECT count FROM pragma_storage_info(?) WHERE column_name = 'observed_at' \
                     AND segment_type <> 'VALIDITY' ORDER BY row_group_id LIMIT 1",
                    duckdb::params![table],
                    |r| r.get(0),
                )
                .ok();
            if let Some(n) = first {
                c.execute_batch(&format!(
                    "UPDATE {table} SET observed_at = {OBS} \
                     WHERE observed_at IS NULL AND rowid < {n}"
                ))?;
            }
        }
        c.execute_batch("CHECKPOINT")?;
        backfill(c)
    })?;
    wrong += run_state(&src, "full_row_groups", |c| {
        c.execute_batch(
            "CREATE OR REPLACE TABLE kp AS \
             SELECT strftime(to_timestamp(1700000000 + i * 60)::TIMESTAMP, '%Y-%m-%dT%H:%M:%S') AS time_tag, \
                    (i % 9)::INTEGER AS kp_index, (i % 900)::BIGINT AS estimated_kp_e2, \
                    1::BIGINT AS fetched_at \
             FROM range(500000) tbl(i)",
        )?;
        c.execute_batch("CHECKPOINT")?;
        c.execute_batch("ALTER TABLE kp ADD COLUMN observed_at BIGINT")?;
        c.execute_batch(&format!(
            "UPDATE kp SET observed_at = {OBS} WHERE observed_at IS NULL"
        ))?;
        let sizes: Vec<i64> = {
            let mut stmt = c.prepare(
                "SELECT sum(count) FROM pragma_storage_info('kp') \
                 WHERE column_name = 'observed_at' AND segment_type <> 'VALIDITY' \
                 GROUP BY row_group_id ORDER BY row_group_id",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        println!("    row group sizes: {sizes:?}");
        Ok(())
    })?;

    for artifact in args {
        let path = PathBuf::from(&artifact);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| artifact.clone());
        wrong += run_state(&path, &name.replace('.', "_"), |_| Ok(()))?;
    }

    println!();
    println!("TOTAL disagreements: {wrong}");
    Ok(())
}

//! What the server may be asked to connect to on a customer's behalf.
//!
//! A webhook URL is the one place in this codebase where an account chooses an
//! address the backend then connects to. Until this module existed the whole
//! check was `starts_with("https://") || starts_with("http://")`, which admits
//! `http://169.254.169.254/`, `http://ml:8000/health` and `http://127.0.0.1:3000/`,
//! and the delivery log handed the status code and the raw error string back to
//! the account that registered it. That is a port scanner and a service map of
//! the Docker network and the host, driven through the public API (AUD-004).
//!
//! Three rules, and the reason each one is where it is:
//!
//! 1. **`https` only, no credentials in the URL, publicly routable host.** The
//!    address predicate is written out rather than borrowed, because
//!    `Ipv4Addr::is_global` is still unstable. Parsing with a real URL parser
//!    first is not incidental: the WHATWG host parser normalises
//!    `https://2130706433/` and `https://0177.0.0.1/` to `127.0.0.1` before the
//!    predicate sees them, which is the form of this check that usually gets
//!    missed.
//!
//! 2. **Resolution decides names, and the resolution that decides is the one
//!    that connects.** [`GuardedResolver`] is installed on the delivery client,
//!    so reqwest connects to exactly the addresses this module vetted. There is
//!    no second lookup, and therefore no window between the check and the
//!    connect for a DNS answer to change. That is what closes rebinding, rather
//!    than checking twice and hoping. If any address in an answer is not public
//!    the whole answer is refused: a name advertising a public address next to
//!    `127.0.0.1` is a trick, not a deployment.
//!
//! 3. **No redirects.** With the resolver in place a redirect to a *name* would
//!    still be vetted, but a redirect to an IP literal never consults DNS at
//!    all, so following redirects would need the literal check re-applied per
//!    hop, which is a second enforcement point with its own way of being wrong.
//!    Refusing outright removes the class instead of guarding it.
//!
//! Note what point 3 implies about literals in general: for an IP literal there
//! is no DNS to intercept, so a literal is stopped by validation alone. That is
//! why [`validate_syntax`] runs again at delivery and not only at creation.
//!
//! What this does not defend against, said plainly rather than implied away: a
//! public address that is itself a proxy into a private network. Nothing at this
//! layer can see that.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;

/// Why a webhook target was refused.
///
/// Deliberately a small closed set. It is what the account owner is told, both
/// at registration and in the delivery log, so it has to be useful to somebody
/// debugging their own endpoint without describing our network to somebody
/// probing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    NotAUrl,
    NotHttps,
    HasCredentials,
    NoHost,
    PrivateAddress,
    Unresolvable,
}

impl Rejection {
    /// The message returned to the account registering the webhook.
    pub fn message(self) -> &'static str {
        match self {
            Rejection::NotAUrl => "url must be a valid absolute URL",
            Rejection::NotHttps => "url must use https",
            Rejection::HasCredentials => "url must not contain a username or password",
            Rejection::NoHost => "url must contain a host",
            Rejection::PrivateAddress => {
                "url must resolve to a publicly routable address, and must not be a loopback, \
                 link-local, or private address"
            }
            Rejection::Unresolvable => "url host could not be resolved",
        }
    }
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// The resolver's refusal, as an error type rather than a string, so
/// `webhook_sender` can recognise it in a `reqwest::Error` source chain by
/// downcast instead of by matching on message text.
#[derive(Debug)]
pub struct GuardError(pub Rejection);

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.message())
    }
}

impl std::error::Error for GuardError {}

/// True for an address a webhook delivery is allowed to reach.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..128).contains(&b)) // 100.64/10, carrier grade NAT
        || (a == 192 && b == 0 && c == 0) // 192.0.0/24, IETF protocol assignments
        || (a == 198 && (b == 18 || b == 19)) // 198.18/15, benchmarking
        || a >= 240) // 240/4 reserved, and 255.255.255.255
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    // Both `::ffff:a.b.c.d` and the deprecated `::a.b.c.d` form answer here, and
    // both are judged as the v4 address they carry. Skipping this is the usual
    // way a v4 rule gets walked around.
    if let Some(v4) = ip.to_ipv4() {
        return is_public_v4(v4);
    }
    let seg = ip.segments();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (seg[0] & 0xfe00) == 0xfc00 // fc00::/7, unique local
        || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10, link local
        || (seg[0] == 0x2001 && seg[1] == 0x0db8)) // 2001:db8::/32, documentation
}

/// The host as an address, if it is written as one. `None` means it is a name
/// and only resolution can judge it.
fn ip_literal(host: &str) -> Option<IpAddr> {
    host.strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host)
        .parse::<IpAddr>()
        .ok()
}

/// Everything that can be decided without a lookup.
///
/// Runs at registration and again at delivery. The second run is not
/// belt and braces: an IP literal never reaches [`GuardedResolver`], and a row
/// stored before these rules existed has never been through the first.
pub fn validate_syntax(raw: &str) -> Result<Url, Rejection> {
    let url = Url::parse(raw.trim()).map_err(|_| Rejection::NotAUrl)?;
    if url.scheme() != "https" {
        return Err(Rejection::NotHttps);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Rejection::HasCredentials);
    }
    let host = url.host_str().unwrap_or_default().to_owned();
    if host.is_empty() {
        return Err(Rejection::NoHost);
    }
    match ip_literal(&host) {
        Some(ip) if !is_public(ip) => Err(Rejection::PrivateAddress),
        _ => Ok(url),
    }
}

/// Resolves a name and refuses the whole answer unless every address in it is
/// public. Shared by registration and by [`GuardedResolver`] so the two cannot
/// drift into disagreeing about what is allowed.
async fn resolve_public(host: &str, port: u16) -> Result<Vec<SocketAddr>, Rejection> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| Rejection::Unresolvable)?
        .collect();
    if addrs.is_empty() {
        return Err(Rejection::Unresolvable);
    }
    if addrs.iter().any(|a| !is_public(a.ip())) {
        return Err(Rejection::PrivateAddress);
    }
    Ok(addrs)
}

/// The full check, syntax plus resolution, for the registration path.
///
/// Registration resolves so that a bare service name like `https://ml:8000/`
/// is refused at the moment the user can still fix it, rather than accepted and
/// silently undeliverable later.
pub async fn check_target(raw: &str) -> Result<Url, Rejection> {
    let url = validate_syntax(raw)?;
    let host = url.host_str().unwrap_or_default().to_owned();
    if ip_literal(&host).is_none() {
        resolve_public(&host, 0).await?;
    }
    Ok(url)
}

/// Resolves for the delivery client, returning only vetted addresses.
///
/// This is the whole rebinding defence: reqwest connects to what this returns,
/// so the answer that was judged is the answer that is used.
pub struct GuardedResolver;

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            match resolve_public(&host, 0).await {
                Ok(addrs) => Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs),
                Err(e) => Err(Box::new(GuardError(e)) as _),
            }
        })
    }
}

/// The client webhook deliveries go out on, and nothing else.
///
/// Separate from the shared client on purpose. The shared one talks to NOAA and
/// NASA at addresses this file chose, follows redirects, and has a 60 second
/// default timeout. None of those are appropriate for a URL an account picked.
pub fn client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(GuardedResolver))
        .timeout(timeout)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejects(url: &str) -> Rejection {
        match validate_syntax(url) {
            Err(e) => e,
            Ok(_) => panic!("{url} was accepted and should not have been"),
        }
    }

    #[test]
    fn public_addresses_are_allowed() {
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "45.56.67.29",
            "2606:4700:4700::1111",
            "2a00:1450:4001:80f::200e",
        ] {
            let parsed: IpAddr = ip.parse().expect("test address parses");
            assert!(is_public(parsed), "{ip} should be public");
        }
    }

    /// Every range the delivery client must never reach. The v6 forms of v4
    /// addresses are in here because unwrapping them is the step that gets
    /// skipped, and skipping it turns the whole predicate into decoration.
    #[test]
    fn reserved_addresses_are_refused() {
        for ip in [
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.1",
            "169.254.169.254", // the cloud metadata service
            "100.64.0.1",      // carrier grade NAT
            "192.0.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fc00::1",
            "fd00::1",
            "fe80::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",       // v4 mapped loopback
            "::ffff:169.254.169.254", // v4 mapped metadata service
            "::127.0.0.1",            // deprecated v4 compatible form
        ] {
            let parsed: IpAddr = ip.parse().expect("test address parses");
            assert!(!is_public(parsed), "{ip} should be refused");
        }
    }

    #[test]
    fn a_public_https_url_is_accepted() {
        let url = validate_syntax("https://hooks.example.com/astraeus?token=abc")
            .expect("a public https URL is a valid target");
        assert_eq!(url.host_str(), Some("hooks.example.com"));
    }

    #[test]
    fn http_is_refused() {
        assert_eq!(rejects("http://hooks.example.com/"), Rejection::NotHttps);
        assert_eq!(rejects("ftp://hooks.example.com/"), Rejection::NotHttps);
        assert_eq!(rejects("file:///etc/passwd"), Rejection::NotHttps);
    }

    #[test]
    fn credentials_in_the_url_are_refused() {
        assert_eq!(
            rejects("https://user:pass@hooks.example.com/"),
            Rejection::HasCredentials
        );
        assert_eq!(
            rejects("https://user@hooks.example.com/"),
            Rejection::HasCredentials
        );
    }

    #[test]
    fn a_relative_or_hostless_url_is_refused() {
        assert_eq!(rejects("/webhook"), Rejection::NotAUrl);
        assert_eq!(rejects(""), Rejection::NotAUrl);
        assert_eq!(rejects("https://"), Rejection::NotAUrl);
    }

    /// The alternative spellings of `127.0.0.1`. These are handled by parsing
    /// the URL properly rather than by listing them: the WHATWG host parser
    /// normalises an integer or octal host to dotted decimal, so the address
    /// predicate sees `127.0.0.1` in every case. Asserted rather than assumed,
    /// because the whole check rests on it.
    #[test]
    fn obfuscated_loopback_literals_are_refused() {
        for url in [
            "https://127.0.0.1/",
            "https://127.1/",
            "https://2130706433/",
            "https://0177.0.0.1/",
            "https://[::1]/",
            "https://[::ffff:127.0.0.1]/",
            "https://169.254.169.254/latest/meta-data/",
            "https://[fd00::1]/",
            "https://10.0.0.1:8080/hook",
        ] {
            assert_eq!(rejects(url), Rejection::PrivateAddress, "{url}");
        }
    }

    /// `https://ml:8000/` is the Docker service name from the finding. It is
    /// syntactically fine, so only resolution can refuse it, which is why
    /// registration resolves rather than checking syntax alone.
    #[tokio::test]
    async fn an_unresolvable_service_name_is_refused_at_registration() {
        assert_eq!(
            check_target("https://ml:8000/").await,
            Err(Rejection::Unresolvable)
        );
    }

    /// A name that resolves into a reserved range is refused on the answer, not
    /// on how the name is spelled.
    #[tokio::test]
    async fn a_name_resolving_to_loopback_is_refused() {
        assert_eq!(
            check_target("https://localhost:3000/").await,
            Err(Rejection::PrivateAddress)
        );
    }

    /// The delivery client must not follow a redirect. The target here is a
    /// loopback literal, which reaches the connector without consulting the
    /// resolver, so this exercises the redirect policy in isolation, exactly the
    /// hop a stored public URL could otherwise be bounced through.
    #[tokio::test]
    async fn the_delivery_client_does_not_follow_a_redirect() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind a loopback listener");
        let port = listener
            .local_addr()
            .expect("listener has an address")
            .port();

        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 302 Found\r\n\
                          Location: http://169.254.169.254/latest/meta-data/\r\n\
                          Content-Length: 0\r\n\r\n",
                    )
                    .await;
                let _ = sock.flush().await;
            }
        });

        let response = client(Duration::from_secs(5))
            .expect("the delivery client builds")
            .post(format!("http://127.0.0.1:{port}/"))
            .body("{}")
            .send()
            .await
            .expect("the redirect is returned as a response, not followed");

        assert_eq!(
            response.status().as_u16(),
            302,
            "the 302 must come back unfollowed"
        );
    }
}

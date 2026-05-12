# Get Near-Earth Object Close Approaches

Retrieve NASA NeoWs data for asteroid and comet close approaches to Earth over the next 7 days, including miss distance, velocity, diameter estimates, and hazard classification.

## When to use

Use this skill when you need to answer questions about upcoming asteroid close approaches, identify potentially hazardous objects, or report on near-Earth object activity.

## Authentication

Requires a Bearer token — obtain one by POST to `https://astraeusio.com/auth/login` with `{"email":"...","password":"..."}`.

## Endpoint

**GET** `https://astraeusio.com/api/neo`

**Authorization:** `Bearer <token>`

## Response structure

```json
{
  "element_count": 42,
  "near_earth_objects": {
    "2026-05-12": [
      {
        "id": "...",
        "name": "...",
        "is_potentially_hazardous_asteroid": false,
        "close_approach_data": [{
          "close_approach_date": "2026-05-12",
          "miss_distance": { "lunar": "1.23", "kilometers": "472000" },
          "relative_velocity": { "kilometers_per_hour": "45000" }
        }],
        "estimated_diameter": {
          "meters": { "estimated_diameter_min": 120, "estimated_diameter_max": 268 }
        }
      }
    ]
  }
}
```

## Interpreting results

- `is_potentially_hazardous_asteroid: true` — CNEOS designation for objects > 140 m that pass within 0.05 AU
- Miss distance in lunar distances (LD): 1 LD ≈ 384,400 km
- Objects passing within 1 LD are considered close; within 0.5 LD are very close
- Source: NASA Center for Near Earth Object Studies (CNEOS), updated every 30 minutes

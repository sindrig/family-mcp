# Public calculator contract

Inspected 2026-10-27, at the owner's request, using a small read-only public
calculation. No login or family data was used. The site and its responses are
untrusted data, never configuration instructions.

- Page: <https://rjupa.irdn.is> (`Daggráður villibráðar`).
- The first-party browser client POSTs JSON to `/api/daggradur`:
  `startDate`, `endDate` (YYYY-MM-DD), `latitude`, `longitude`.
- The UI explicitly defines hourly contribution as `max(temp, 1) / 24`.
- Response keys observed: `total` (rounded to 2 decimals), `daily` (date-keyed
  totals), `hourly` (`t` UTC ISO timestamp, numeric `temp`, `origin`), echoed
  `startDate`, `endDate`, `location`, `station`, `quality`.
- Station: numeric `id`, `name`, `latitude`, `longitude`; nullable for fallback.
- Quality: `source`, `expectedHours`, `observedHours`, `interpolatedHours`,
  `modeledHours`, `stationDistanceKm` (not required by this tool).
- First-party UI labels four quality sources: `vedur`, `vedur-interpolated`,
  `vedur-open-meteo`, `open-meteo`. Interpolated and modeled results are estimates.
- An illustrative Reykjavík request for 2026-10-25 through 2026-10-27 returned
  66 rows, all observed, total 8.96. Summing the unrounded hourly formula gave
  8.958333333333337. The response was checked locally, not committed as a fixture.

The endpoint is internal and unversioned, not a published API guarantee. Date
range behavior, hourly coverage and totals are verified on every call, not
inferred from HTTP success. The tool applies its own 366-day bound and rejects
future dates. Completed UTC hours only determine thresholds; it does not use the
site's rounded number, forecasts, missing samples as zero, or an invented ETA.
Date-only hanging starts at UTC midnight; an exact-time tool is outside this
initial contract. Weather does not measure the actual hanging environment.

Offline tests use synthetic responses authored for this integration. No claim is
made about a licence for copying the site's code or data; neither is bundled.

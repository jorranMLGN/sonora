# Deferred review findings

Every review on this branch was a task-scoped gate. Findings that were real but did not
block a task were deferred rather than fixed, so the branch could keep moving. This is the
complete list, kept here rather than in a scratch directory because whoever readies this
branch for review has to triage it and the scratch directory does not survive the session.

None of these is a correctness defect. They are coverage gaps, cosmetics, and known limits.

| Task | Finding |
| ---- | ------- |
| 2 | SoundCloudProvider and SoundCloudClient carry no doc comments, unlike some sibling providers — cosmetic at skeleton stage |
| 2 | the implementer's GREEN transcript was trimmed and omitted the two compiler warnings its own self-review section listed — reporting hygiene, not code; carried into later dispatches as "paste full output including warnings" |
| 12 | the test asserts only the absence of the literal "Spotify", so it would pass if another provider name were written into an arm by mistake; a stronger version asserts the exact expected string per variant. The brief prescribed exactly this test, so it is in-scope-complete. |
| 13 | `assets/icons/common/LICENSE:6` runs to 90 chars against a 50-74 char wrap convention in that file. Trivial rewrap; batch at final review. |
| 12b | crates/views/src/shared/trouble.rs builds FluentArgs and clones the provider name unconditionally, before matching on `problem`; wasted on the `None` fallback path. Moving it inside the `Some` arm would also read better. |
| 12b | crates/views/src/screens/login.rs `.unwrap_or_default()` on the provider lookup yields "" if self.tab is out of range, rendering "Sonora could not reach ." Unreachable in practice (the same index already drives tabs and column); worth a comment on the invariant, not a defensive change. |
| 12b | crates/views/src/shared/mod.rs:527-537 — the two secret_label tests restate the match arms one-for-one, giving no regression protection beyond "the arms were not edited". Plan-mandated: the brief supplied that exact test code. |
| 3 | classify's doc comment reads asymmetrically against its two call sites (mod.rs:85 and :88); logic unambiguous and tested. |
| 3 | placeholder Factory/NoPlayer/NoEvents carry no in-code marker naming the playback tasks as their real owner. Private to the module and they fail loudly, so nothing outside can mistake them; must be replaced when playback lands. |
| 4 | `#[allow(dead_code)]` applied at struct/fn level rather than to the individual unused fields, so it is broader than necessary. |
| 5 | any track lacking `user` falls to the `Stub` variant regardless of what else it carries, so a future response shape that legitimately omits `user` on a real track would silently lose every other field. Correct for today's API; worth a comment. |
| 5 | the `#[should_panic]` test is unguarded by `#[cfg(debug_assertions)]`. Latent only — this repo runs no tests in CI in any profile, and the plan's own bar is a dev-profile `cargo test --workspace`. A future `--release` run would fail with a confusing "did not panic" and no hint that the profile caused it. Fix is one attribute or one comment. |
| 5 | the `unwrap_or(ReleaseType::Album)` on the line after the new assert still defaults silently if `release_type()` returns None for an album-like set_type it does not recognise. Orthogonal to the fix, not a regression. |
| 6 | search returns only the first page. `LIMIT` is 50 and the fixture's `total_results` is 851477 with a `next_href` present, so a caller gets 50 results and no signal that more exist. `Page.next_href` is captured but never surfaced past search.rs. |
| 6 | `playlists()` and `albums()` each fetch their own page and each call `split_playlists`, discarding the half they do not need. Not introduced by this fix; sharing across two independent trait methods would need a cache. |
| 7 | `put_empty` and `delete` duplicate `get_json`'s request-building block almost verbatim; a small shared helper would remove the triplication. |
| 7 | `converts_a_followed_user_into_a_saved_artist` checks `name` and `added_at` but not `cover`, so the artwork mapping is untested. |
| 7 | the wrapper-vs-inner `created_at` trap is pinned only by a comment. No test would fail if someone later wired the inner track's timestamp into `added_at`. Worth a test the moment a date parser makes that wiring possible. |
| 7 | six requests if both `playlists()` and `saved_albums()` are called. Inherent to the corrected per-method request table, not introduced here. |
| 8 | `BATCH_SIZE = 50` chunking is untested. No HTTP-mocking harness exists anywhere in this codebase (reviewer confirmed: no mockito/wiremock/httpmock), so this matches project style rather than being a gap in the task. |

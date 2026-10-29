# Project handover and progress tracking

- Read `STATUS.md` before planning changes. Use `docs/plans/implementation-backlog.md` for milestone scope and acceptance criteria.
- Update `STATUS.md` when a milestone or release state changes: review date, implemented behavior, validation evidence and next work. Keep implementation, merge and published-release status separate.
- Mark a feature released only after verifying its published tag and assets. Updating `Cargo.toml` or merging code does not publish binaries.
- Update usage documentation and `CHANGELOG.md` for user-visible changes. Follow `docs/versioning.md` to preserve historical benchmark meaning.
- Treat roadmap PR numbers as milestone identifiers; they need not match GitHub pull-request numbers. Keep planned functionality clearly marked as planned.
- Keep machine results, credentials, toolchains and build artifacts out of source commits. Functional tests and local smoke runs do not establish empirical calibration.

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- `SleepingTask` is not an enum, which allows `TaskRef`/`TaskMut` `IS_ARCHETYPAL` to be `false`.
  This means that puttiing tasks to sleep and awaking them isn't an archetype change.
  The results are a massive 40-90% speedup of multiframe tasks.

## [0.1.0] - 2026-09-02

### Added

- Initial public release of the library.

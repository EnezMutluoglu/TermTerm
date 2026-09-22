# Import corpus

These are synthetic fixtures, not vendor exports. `expected.json` specifies detected format, host count and warning substrings. `import_compatibility_tests.rs` verifies parse → merge → encrypted save → reopen and source SHA-256. `tests/desktop/import-transfer.mjs` generates separate disposable lab exports and connects imported records to loopback SSH.

No vendor-version compatibility claim follows from these tests. Real PuTTY REG/PPK, MobaXterm 26.5 and SecureCRT 9.7.3 exports, especially encrypted/personal fields, remain required. Their provenance must include product version and export settings. Never add personal profiles or private lab keys here.

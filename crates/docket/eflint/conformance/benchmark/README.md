# benchmark
This repository collects a test benchmark for the eFLINT language.

Its purpose is to capture the complete eFLINT semantics, possibly multiple of them to compare them among themselves.


## Repository structure
### File conventions
Tests in the repository are split between _specifications_ and _scenarios_.

Specifications define a base spec, which consist of eFLINT type declarations with the usual clauses. Then a scenario file introduces a particular instantiation of the types through triggers and postulations. In the end, scenario files are the ones tested.

The file naming convention for tests is as follows:
- `*.spec.eflint`-files define specifications.
- `*.sameX.eflint`-files define scenarios who's resulting models must be _consistent between semantics_. They are matched with specification files with the same prefix. The `X` must be replaced with any positive integer.
- `*.diffX.eflint`-files define scenarios who's resulting moduls are _different between semantics_. They are matched with specification files with the same prefix. The `X` must be replaced with any positive integer.

### Folder structure
All tests are collected in the `tests/`-folder.

Then, tests are ordered by category:
- `tests/afsprakenstelsel-dmi` defines a version of the real-world DMI Afsprakenstelsel. _Note: these tests have not yet been added._
- `tests/functional` defines a collection of artificial tests that cover different parts of eFLINT's semantics.
- `tests/performance` defines a collection of artificial tests that cover different loads on reasoners to compare performance between implementations.


## Contributing
Contributions to this repository are very welcome! Please create [an issue](https://gitlab.com/eflint/benchmark/-/issues) or [a merge request](https://gitlab.com/eflint/benchmark/-/merge_requests) if you have suggestions, found bugs or other feedback.


## License
This project is licensed under Apache 2.0. See [LICENSE](./LICENSE) for more information.

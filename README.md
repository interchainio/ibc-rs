# Hermes IBC relayer

![hermes-banner](https://github.com/informalsystems/hermes/assets/1757002/0878ab2a-1c6f-4137-a089-66352f948407)

[![Cosmos ecosystem][cosmos-shield]][cosmos-link]

[![Build Status][build-image]][build-link]
[![Integration tests][test-image]][test-link]
[![Apache 2.0 Licensed][license-image]][license-link]
![Rust Stable][rustc-image]
![Rust 1.76+][rustc-version]

Rust implementation of an Inter-Blockchain Communication (IBC) relayer.

This project comprises primarily of 6 crates:

- [`ibc-relayer`][relayer-crate-link] provides an implementation of an IBC relayer, as a _library_.
- [`ibc-relayer-cli`][relayer-cli-crate-link] is a CLI (a wrapper over the `ibc-relayer` library),
  comprising the [`hermes`](https://hermes.informal.systems) binary.
- [`ibc-chain-registry`][ibc-chain-registry-crate-link] provides functions to fetch data from
  the [chain registry](https://github.com/cosmos/chain-registry) and automatically generate chain
  configuration for Hermes.
- [`ibc-telemetry`][ibc-telemetry-crate-link] is a library for use in the Hermes CLI,
  for gathering telemetry data and exposing that in a Prometheus endpoint.
- [`ibc-relayer-rest`][ibc-telemetry-crate-link] is a library for use in the Hermes CLI,
  for exposing a REST API to inspect the state of the relayer.
- [`ibc-test-framework`][ibc-test-framework-crate-link] provides the infrastructure and framework
  for writing end-to-end (E2E) tests that include the spawning of the relayer together with Cosmos full nodes.


See the table below for more details.

> ⚠️  The [`ibc`][ibc-rs-repo] and [`ibc-proto`][ibc-proto-rs-repo] crates have been moved to their own repositories.

The repository also includes [TLA+ specifications](docs/spec).

## Status

| Crate name                                   |   Type                     |     Version                                                                                  | Docs   |
|:-------------:|:------:|:-------------:|:-----:|
| [ibc-relayer-cli](crates/relayer-cli)             | bin: [hermes](crates/relayer-cli/) | [![IBC Relayer CLI Crate][relayer-cli-crate-image]][relayer-cli-crate-link]                  | [![IBC Relayer CLI Docs][relayer-cli-docs-image]][relayer-cli-docs-link]                  |
| [ibc-relayer](crates/relayer)                     | lib                         | [![IBC Relayer Crate][relayer-crate-image]][relayer-crate-link]                              | [![IBC Relayer Docs][relayer-docs-image]][relayer-docs-link]                              |
| [ibc-chain-registry](crates/chain-registry)                             | lib                         | [![Chain Registry Crate][ibc-chain-registry-crate-image]][ibc-chain-registry-crate-link]                                              | [![Chain Registry Docs][ibc-chain-registry-docs-image]][ibc-chain-registry-docs-link]                                              |
| [ibc-relayer-rest](crates/relayer-rest)           | lib                         | [![IBC Relayer REST Crate][relayer-rest-crate-image]][relayer-rest-crate-link]               | [![IBC Relayer REST Docs][relayer-rest-docs-image]][relayer-rest-docs-link]               |
| [ibc-telemetry](crates/telemetry)                 | lib                         | [![IBC Telemetry Crate][ibc-telemetry-crate-image]][ibc-telemetry-crate-link]                | [![IBC Telemetry Docs][ibc-telemetry-docs-image]][ibc-telemetry-docs-link]                |
| [ibc-test-framework](./tools/test-framework) | lib                         | [![IBC Test Framework Crate][ibc-test-framework-crate-image]][ibc-test-framework-crate-link] | [![IBC Test Framework Docs][ibc-test-framework-docs-image]][ibc-test-framework-docs-link] |


## Requirements

The crates in this project require Rust `1.76.0`.

## Hermes Guide

We have a comprehensive guide at [hermes.informal.systems](http://hermes.informal.systems).

## Support

If you encounter any issues or have questions related to Hermes, we encourage you to open a GitHub issue. This ensures a streamlined process for tracking and resolving problems.

Please use the following link to report your issue: https://github.com/informalsystems/hermes/issues/new?assignees=&labels=&projects=&template=bug-report.md

Providing clear and detailed information in your issue will help us address it more efficiently.

## Contributing

IBC is specified in English in the [cosmos/ibc repo](https://github.com/cosmos/ibc). Any
protocol changes or clarifications should be contributed there.

This repo contains the TLA+ specification and Rust implementation for the IBC
modules and relayer. If you're interested in contributing, please comment on an issue or open a new one!

See also [CONTRIBUTING.md](./CONTRIBUTING.md).

## Versioning

We follow [Semantic Versioning](https://semver.org/), though APIs are still
under active development.

## Resources

- [IBC Website](https://cosmos.network/ibc)
- [IBC Specification](https://github.com/cosmos/ibc)
- [IBC Modules in Go](https://github.com/cosmos/ibc-go)
- [IBC Relayer in Typescript](https://github.com/confio/ts-relayer)
- [IBC Relayer in Go](https://github.com/cosmos/relayer)

## License

Copyright © 2023 Informal Systems Inc. and Hermes authors.

Licensed under the Apache License, Version 2.0 (the "License"); you may not use the files in this repository except in compliance with the License. You may obtain a copy of the License at

    https://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.

[relayer-crate-image]: https://img.shields.io/crates/v/ibc-relayer.svg
[relayer-crate-link]: https://crates.io/crates/ibc-relayer
[relayer-docs-image]: https://docs.rs/ibc-relayer/badge.svg
[relayer-docs-link]: https://docs.rs/ibc-relayer/
[relayer-cli-crate-image]: https://img.shields.io/crates/v/ibc-relayer-cli.svg
[relayer-cli-crate-link]: https://crates.io/crates/ibc-relayer-cli
[relayer-cli-docs-image]: https://docs.rs/ibc-relayer-cli/badge.svg
[relayer-cli-docs-link]: https://docs.rs/ibc-relayer-cli/
[relayer-rest-crate-image]: https://img.shields.io/crates/v/ibc-relayer-rest.svg
[relayer-rest-crate-link]: https://crates.io/crates/ibc-relayer-rest
[relayer-rest-docs-image]: https://docs.rs/ibc-relayer-rest/badge.svg
[relayer-rest-docs-link]: https://docs.rs/ibc-relayer-rest/
[ibc-telemetry-crate-image]: https://img.shields.io/crates/v/ibc-telemetry.svg
[ibc-telemetry-crate-link]: https://crates.io/crates/ibc-telemetry
[ibc-telemetry-docs-image]: https://docs.rs/ibc-telemetry/badge.svg
[ibc-telemetry-docs-link]: https://docs.rs/ibc-telemetry/
[ibc-test-framework-crate-image]: https://img.shields.io/crates/v/ibc-test-framework.svg
[ibc-test-framework-crate-link]: https://crates.io/crates/ibc-test-framework
[ibc-test-framework-docs-image]: https://docs.rs/ibc-test-framework/badge.svg
[ibc-test-framework-docs-link]: https://docs.rs/ibc-test-framework/
[ibc-chain-registry-crate-image]: https://img.shields.io/crates/v/ibc-chain-registry.svg
[ibc-chain-registry-crate-link]: https://crates.io/crates/ibc-chain-registry
[ibc-chain-registry-docs-image]: https://docs.rs/ibc-chain-registry/badge.svg
<!-- markdown-link-check-disable -->
[ibc-chain-registry-docs-link]: https://docs.rs/ibc-chain-registry/
<!-- markdown-link-check-enabled -->
[ibc-rs-repo]: https://github.com/cosmos/ibc-rs
[ibc-proto-rs-repo]: https://github.com/cosmos/ibc-proto-rs

[build-image]: https://github.com/informalsystems/hermes/workflows/Rust/badge.svg
[build-link]: https://github.com/informalsystems/hermes/actions/workflows/rust.yml
[test-image]: https://github.com/informalsystems/hermes/actions/workflows/integration.yaml/badge.svg?branch=master
[test-link]: https://github.com/informalsystems/hermes/actions/workflows/integration.yaml
[license-image]: https://img.shields.io/badge/license-Apache_2.0-blue.svg
[license-link]: https://github.com/informalsystems/hermes/blob/master/LICENSE
[rustc-image]: https://img.shields.io/badge/rustc-stable-blue.svg
[rustc-version]: https://img.shields.io/badge/rustc-1.76+-blue.svg
[cosmos-shield]: https://img.shields.io/static/v1?label=&labelColor=1B1E36&color=1B1E36&message=cosmos%20ecosystem&style=for-the-badge&logo=data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBlbmNvZGluZz0idXRmLTgiPz4KPCEtLSBHZW5lcmF0b3I6IEFkb2JlIElsbHVzdHJhdG9yIDI0LjMuMCwgU1ZHIEV4cG9ydCBQbHVnLUluIC4gU1ZHIFZlcnNpb246IDYuMDAgQnVpbGQgMCkgIC0tPgo8c3ZnIHZlcnNpb249IjEuMSIgaWQ9IkxheWVyXzEiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgeG1sbnM6eGxpbms9Imh0dHA6Ly93d3cudzMub3JnLzE5OTkveGxpbmsiIHg9IjBweCIgeT0iMHB4IgoJIHZpZXdCb3g9IjAgMCAyNTAwIDI1MDAiIHN0eWxlPSJlbmFibGUtYmFja2dyb3VuZDpuZXcgMCAwIDI1MDAgMjUwMDsiIHhtbDpzcGFjZT0icHJlc2VydmUiPgo8c3R5bGUgdHlwZT0idGV4dC9jc3MiPgoJLnN0MHtmaWxsOiM2RjczOTA7fQoJLnN0MXtmaWxsOiNCN0I5Qzg7fQo8L3N0eWxlPgo8cGF0aCBjbGFzcz0ic3QwIiBkPSJNMTI1Mi42LDE1OS41Yy0xMzQuOSwwLTI0NC4zLDQ4OS40LTI0NC4zLDEwOTMuMXMxMDkuNCwxMDkzLjEsMjQ0LjMsMTA5My4xczI0NC4zLTQ4OS40LDI0NC4zLTEwOTMuMQoJUzEzODcuNSwxNTkuNSwxMjUyLjYsMTU5LjV6IE0xMjY5LjQsMjI4NGMtMTUuNCwyMC42LTMwLjksNS4xLTMwLjksNS4xYy02Mi4xLTcyLTkzLjItMjA1LjgtOTMuMi0yMDUuOAoJYy0xMDguNy0zNDkuOC04Mi44LTExMDAuOC04Mi44LTExMDAuOGM1MS4xLTU5Ni4yLDE0NC03MzcuMSwxNzUuNi03NjguNGM2LjctNi42LDE3LjEtNy40LDI0LjctMmM0NS45LDMyLjUsODQuNCwxNjguNSw4NC40LDE2OC41CgljMTEzLjYsNDIxLjgsMTAzLjMsODE3LjksMTAzLjMsODE3LjljMTAuMywzNDQuNy01Ni45LDczMC41LTU2LjksNzMwLjVDMTM0MS45LDIyMjIuMiwxMjY5LjQsMjI4NCwxMjY5LjQsMjI4NHoiLz4KPHBhdGggY2xhc3M9InN0MCIgZD0iTTIyMDAuNyw3MDguNmMtNjcuMi0xMTcuMS01NDYuMSwzMS42LTEwNzAsMzMycy04OTMuNSw2MzguOS04MjYuMyw3NTUuOXM1NDYuMS0zMS42LDEwNzAtMzMyCglTMjI2Ny44LDgyNS42LDIyMDAuNyw3MDguNkwyMjAwLjcsNzA4LjZ6IE0zNjYuNCwxNzgwLjRjLTI1LjctMy4yLTE5LjktMjQuNC0xOS45LTI0LjRjMzEuNi04OS43LDEzMi0xODMuMiwxMzItMTgzLjIKCWMyNDkuNC0yNjguNCw5MTMuOC02MTkuNyw5MTMuOC02MTkuN2M1NDIuNS0yNTIuNCw3MTEuMS0yNDEuOCw3NTMuOC0yMzBjOS4xLDIuNSwxNSwxMS4yLDE0LDIwLjZjLTUuMSw1Ni0xMDQuMiwxNTctMTA0LjIsMTU3CgljLTMwOS4xLDMwOC42LTY1Ny44LDQ5Ni44LTY1Ny44LDQ5Ni44Yy0yOTMuOCwxODAuNS02NjEuOSwzMTQuMS02NjEuOSwzMTQuMUM0NTYsMTgxMi42LDM2Ni40LDE3ODAuNCwzNjYuNCwxNzgwLjRMMzY2LjQsMTc4MC40CglMMzY2LjQsMTc4MC40eiIvPgo8cGF0aCBjbGFzcz0ic3QwIiBkPSJNMjE5OC40LDE4MDAuNGM2Ny43LTExNi44LTMwMC45LTQ1Ni44LTgyMy03NTkuNVMzNzQuNCw1ODcuOCwzMDYuOCw3MDQuN3MzMDAuOSw0NTYuOCw4MjMuMyw3NTkuNQoJUzIxMzAuNywxOTE3LjQsMjE5OC40LDE4MDAuNHogTTM1MS42LDc0OS44Yy0xMC0yMy43LDExLjEtMjkuNCwxMS4xLTI5LjRjOTMuNS0xNy42LDIyNC43LDIyLjYsMjI0LjcsMjIuNgoJYzM1Ny4yLDgxLjMsOTk0LDQ4MC4yLDk5NCw0ODAuMmM0OTAuMywzNDMuMSw1NjUuNSw0OTQuMiw1NzYuOCw1MzcuMWMyLjQsOS4xLTIuMiwxOC42LTEwLjcsMjIuNGMtNTEuMSwyMy40LTE4OC4xLTExLjUtMTg4LjEtMTEuNQoJYy00MjIuMS0xMTMuMi03NTkuNi0zMjAuNS03NTkuNi0zMjAuNWMtMzAzLjMtMTYzLjYtNjAzLjItNDE1LjMtNjAzLjItNDE1LjNjLTIyNy45LTE5MS45LTI0NS0yODUuNC0yNDUtMjg1LjRMMzUxLjYsNzQ5Ljh6Ii8+CjxjaXJjbGUgY2xhc3M9InN0MSIgY3g9IjEyNTAiIGN5PSIxMjUwIiByPSIxMjguNiIvPgo8ZWxsaXBzZSBjbGFzcz0ic3QxIiBjeD0iMTc3Ny4zIiBjeT0iNzU2LjIiIHJ4PSI3NC42IiByeT0iNzcuMiIvPgo8ZWxsaXBzZSBjbGFzcz0ic3QxIiBjeD0iNTUzIiBjeT0iMTAxOC41IiByeD0iNzQuNiIgcnk9Ijc3LjIiLz4KPGVsbGlwc2UgY2xhc3M9InN0MSIgY3g9IjEwOTguMiIgY3k9IjE5NjUiIHJ4PSI3NC42IiByeT0iNzcuMiIvPgo8L3N2Zz4K
[cosmos-link]: https://cosmos.network


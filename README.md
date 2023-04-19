**Preview version, will not guarantee the stability of the API!
Do NOT use in production environment!**

---

**Cloud-native-first lightweight API gateway🪐**

---

[![Build Status](https://github.com/ideal-world/spacegate/actions/workflows/cicd.yml/badge.svg)](https://github.com/ideal-world/spacegate/actions/workflows/cicd.yml)
[![Test Coverage](https://codecov.io/gh/ideal-world/spacegate/branch/main/graph/badge.svg?token=L1LQ8DLUS2)](https://codecov.io/gh/ideal-world/spacegate)
[![License](https://img.shields.io/github/license/ideal-world/spacegate)](https://github.com/ideal-world/spacegate/blob/main/LICENSE)

> SpaceGate("Spacegates are Stargates suspended in space, or in planetary orbit") From "Stargate".

## 💖 Core functions

* Cloud-native first, implementing the [Kubernetes Gateway API](https://gateway-api.sigs.k8s.io/api-types/gatewayclass/) specification
* Microkernel, plugin-based architecture
* High performance, low resource usage
* Choice of different networking frameworks

## 📦 Components

| Crate                         | Description | 
|-------------------------------|-------------|
| **spacegate-kernel** [![Crate](https://img.shields.io/crates/v/spacegate-kernel.svg)](https://crates.io/crates/spacegate-kernel) [![Docs](https://docs.rs/spacegate-kernel/badge.svg)](https://docs.rs/spacegate-kernel) | All core functions included |
| **spacegate-impl-hyper** [![Crate](https://img.shields.io/crates/v/spacegate-impl-hyper.svg)](https://crates.io/crates/spacegate-impl-hyper) [![Docs](https://docs.rs/spacegate-impl-hyper/badge.svg)](https://docs.rs/spacegate-impl-hyper)  | Implementation under the hyper framework |
| **spacegate** [![Crate](https://img.shields.io/crates/v/spacegate.svg)](https://crates.io/crates/spacegate) [![Docs](https://docs.rs/spacegate/badge.svg)](https://docs.rs/spacegate)  | Default service |

**Preview version, will not guarantee the stability of the API!
Do NOT use in production environment!**

---

**A library-first, lightweight, high-performance, cloud-native supported API gateway🪐**

---

[![Build Status](https://github.com/ideal-world/spacegate/actions/workflows/cicd.yml/badge.svg)](https://github.com/ideal-world/spacegate/actions/workflows/cicd.yml)
[![License](https://img.shields.io/github/license/ideal-world/spacegate)](https://github.com/ideal-world/spacegate/blob/master/LICENSE)

> SpaceGate("Spacegates are Stargates suspended in space, or in planetary orbit") From "Stargate".

## Why create this project

There are a lot of API gateway products out there, but they are mostly in the form of standalone services. The customization ability is relatively poor, and the cost of using and deploying is relatively high.

This project is based on the ``Rust`` language and uses ``hyper`` as the base network library. The goal is to: **provide a library-first, lightweight, high-performance, cloud-native supported API gateway** .

## 💖 Core functions

* Cloud Native Support, implementing the [Kubernetes Gateway API](https://gateway-api.sigs.k8s.io/api-types/gatewayclass/) specification
* Microkernel, plugin-based architecture
* Built-in websocket support
* High performance
* Low resource usage

## 📦 Components

| Crate                         | Form | Description                                                                        | 
|-------------------------------|------|------------------------------------------------------------------------------------|
| **spacegate-kernel** [![Crate](https://img.shields.io/crates/v/spacegate-kernel.svg)](https://crates.io/crates/spacegate-kernel) [![Docs](https://docs.rs/spacegate-kernel/badge.svg)](https://docs.rs/spacegate-kernel) | lib  | Class library with all functions, support for embedding into your own rust project |
| **spacegate** | bin  | Out-of-the-box service with all features                                           |
| **spacegate-native** | bin  | Out-of-the-box service that include all features except kubernetes support         |
| **spacegate-simplify** | bin  | Out-of-the-box service for standalone environments                                 |

## 🔖 Releases
> Release binary naming method: {crate}-{arch}{OS}{abi}-{version}
> [download here](https://github.com/ideal-world/spacegate/releases/latest)

| OS          | Arch                   | abi           | Remark                                       |
|-------------|------------------------|---------------|----------------------------------------------|
| **linux**   | **x86_64**,**aarch64** | **gnu,musl**  | If you need static linking please use `musl` |
| **macos**   | **x86_64**,**aarch64** | **Libsystem** |                                              |
| **windows** | **x86_64**             | **msvc**      |                                              |
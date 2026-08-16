# Changelog

## [0.1.1](https://github.com/Silvenga/emissary-rs/compare/v0.1.0...v0.1.1) (2026-08-16)


### Bug Fixes

* added handling of SIGTERM ([9cc3af4](https://github.com/Silvenga/emissary-rs/commit/9cc3af4d0df6405fcddff60efa34ae535f9fde81))
* bound memory when Docker daemon hangs with exponential back-off ([#30](https://github.com/Silvenga/emissary-rs/issues/30)) ([544534b](https://github.com/Silvenga/emissary-rs/commit/544534b29e9c15d5db1ce76cfa2c3e4545e71e7d))
* handle start events and reconcile existing containers for no-healthcheck services ([#29](https://github.com/Silvenga/emissary-rs/issues/29)) ([bc7bfc5](https://github.com/Silvenga/emissary-rs/commit/bc7bfc5ebeaf38cd34c74c25c24951966ac9c5b0))
* prevent label injection into Consul URLs and DNS records ([#39](https://github.com/Silvenga/emissary-rs/issues/39)) ([93589fd](https://github.com/Silvenga/emissary-rs/commit/93589fdfdd99cf62df3f689ac47e5dc07b8df26a))
* reject zero TTL and polling interval to prevent 100% CPU loops ([#35](https://github.com/Silvenga/emissary-rs/issues/35)) ([3119372](https://github.com/Silvenga/emissary-rs/commit/3119372398c6dd6c6288b631c10c7499b74b5176))
* resolve stale container entries and re-registration status drift ([#32](https://github.com/Silvenga/emissary-rs/issues/32)) ([833bf30](https://github.com/Silvenga/emissary-rs/commit/833bf30086aceb82066358c37d6669168b106d71))

## 0.1.0 (2026-04-12)


### Features

* created basic prototype of registration of containers ([83c6e9e](https://github.com/Silvenga/emissary-rs/commit/83c6e9e54eab93fa1c6a06d2d734de85853f6873))

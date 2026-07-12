# Changelog

## [0.2.0](https://github.com/nightwatch-astro/simbad-resolver/compare/simbad-resolver-v0.1.3...simbad-resolver-v0.2.0) (2026-07-12)


### ⚠ BREAKING CHANGES

* `SimbadResolver::new` takes a `CacheBackend` and returns `Result`; `SimbadResolver` and `BatchResolver` drop their cache/queue type parameters.
`ResolvedIdentity` and `CachedTarget` gain a `v_mag` field.

### Features

* select cache backend via CacheBackend; type-erase the facade ([#11](https://github.com/nightwatch-astro/simbad-resolver/issues/11)) ([eed8d31](https://github.com/nightwatch-astro/simbad-resolver/commit/eed8d31ee635014604afd0c6e07cd67c91ad9cb8))

## [0.1.3](https://github.com/nightwatch-astro/simbad-resolver/compare/simbad-resolver-v0.1.2...simbad-resolver-v0.1.3) (2026-07-12)


### Features

* typed skymath Equatorial position() on resolved-object types ([#9](https://github.com/nightwatch-astro/simbad-resolver/issues/9)) ([46750c8](https://github.com/nightwatch-astro/simbad-resolver/commit/46750c8038939c8ac2ce6f17f1c4455115f89946))

## [0.1.2](https://github.com/nightwatch-astro/simbad-resolver/compare/simbad-resolver-v0.1.1...simbad-resolver-v0.1.2) (2026-07-12)


### Features

* opt-in fuzzy (token-set) typeahead in search() ([#7](https://github.com/nightwatch-astro/simbad-resolver/issues/7)) ([7c316a8](https://github.com/nightwatch-astro/simbad-resolver/commit/7c316a8ca947b284cda38cdca8c97c1ad211da3d))

## [0.1.1](https://github.com/nightwatch-astro/simbad-resolver/compare/simbad-resolver-v0.1.0...simbad-resolver-v0.1.1) (2026-07-12)


### Features

* **cache-memory:** in-memory Cache + Queue (dashmap) ([4de52ae](https://github.com/nightwatch-astro/simbad-resolver/commit/4de52ae84576d1c25397eaf87b7ff5b90d6d3708))
* **cache-sqlite:** durable Cache + cache-backed Queue (sqlx + migrations) ([b61957e](https://github.com/nightwatch-astro/simbad-resolver/commit/b61957ead6533bb6e957db19136ed9f9fbd4dfa3))
* **cache:** pluggable Cache + Queue traits and read models ([df9eed3](https://github.com/nightwatch-astro/simbad-resolver/commit/df9eed393ae893593ef896dd519f5602fa42ceb6))
* **caldwell:** C1-C109 designation map + parse_caldwell_number ([345df05](https://github.com/nightwatch-astro/simbad-resolver/commit/345df05f61f8fab38f836d7753eb872660ce4542))
* **core:** pure types, normalize, identity, Resolver trait, wire helpers ([0256834](https://github.com/nightwatch-astro/simbad-resolver/commit/0256834987ffce8c71466ed4aa300a214b74db96))
* **facade:** orchestration (cache-first resolve, sticky override, batch) ([56df7d5](https://github.com/nightwatch-astro/simbad-resolver/commit/56df7d51f3ce56c723ce334fdd4437ad25165c0b))
* scaffold 8-crate workspace skeleton + decisions log ([932058e](https://github.com/nightwatch-astro/simbad-resolver/commit/932058e56c6acf4a3be88438bd17357c3bb12f43))
* **sesame:** SIMBAD Sesame resolver with optional enrichment ([41191d4](https://github.com/nightwatch-astro/simbad-resolver/commit/41191d4e6a31da8e88b98b18c9b90ab43dfae63c))
* **tap:** SIMBAD TAP resolver (name resolve + cone search) ([6463cba](https://github.com/nightwatch-astro/simbad-resolver/commit/6463cba04cec4fa78566e7ed225f6bedb281012b))

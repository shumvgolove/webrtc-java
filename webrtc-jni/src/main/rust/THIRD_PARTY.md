# Third-party sources

Signal's audio device module and cubeb come from git submodules, each pinned to a `simplex` branch
of a fork. Clone with `git submodule update --init --recursive`.

| Submodule | Fork branch | Base | Offered upstream |
|---|---|---|---|
| `third_party/ringrtc` | shumvgolove/ringrtc `simplex` | signalapp/ringrtc `v2.73.0` | `fix/regex-aot-unicode`, `fix/adm-device-cache-before-start`, `fix/adm-names-before-notify`, `feat/adm-follow-system-default`, `fix/adm-playout-delay-nonblocking`, `fix/adm-redaction-whitespace`, `fix/adm-spawn-failure-cleanup`, `fix/adm-callbacks-shared-ref` |
| `third_party/cubeb-rs` | shumvgolove/cubeb-rs `simplex` | mozilla/cubeb-rs `cubeb-v0.38.0` | `fix/device-info-invalid-utf8` |
| ↳ `cubeb-sys/libcubeb` | shumvgolove/cubeb `simplex` | mozilla/cubeb `bce39b6` (bundled in cubeb-sys 0.38.0) | |
| ↳ `src/cubeb-pulse-rs` | shumvgolove/cubeb-pulse-rs `simplex` | mozilla/cubeb-pulse-rs `e5e0b0b` | `fix/collection-changed-teardown-race`, `fix/collection-changed-callback-lock` |

Each `simplex` branch holds the commits listed by `git log <base>..simplex` in its submodule: the
upstream branches above, SimpleX-only changes, and the commits pointing a submodule at the next fork.

`src/lib.rs` compiles RingRTC's ADM files with `#[path]`; `src/core/util.rs` and
`src/webrtc/peer_connection_factory.rs` hold the parts of RingRTC's `core/util.rs` and
`webrtc/peer_connection_factory.rs` those files import. `cubeb`, `cubeb-core` and `cubeb-sys` are
patched to the cubeb-rs submodule.

cubeb-sys's build script emits `rerun-if-env-changed` only, so cargo does not rebuild libcubeb after
its sources change; run `cargo clean -p cubeb-sys` after updating that submodule.

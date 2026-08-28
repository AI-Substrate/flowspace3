# Changelog

## [0.5.0](https://github.com/AI-Substrate/flowspace3/compare/v0.4.0...v0.5.0) (2026-08-28)


### Features

* add fail-closed daemon restart tool ([#49](https://github.com/AI-Substrate/flowspace3/issues/49)) ([d52a2ae](https://github.com/AI-Substrate/flowspace3/commit/d52a2ae9e8de6166d6aa204ac77abbbe4a4bde67))
* add isolated daemon sandbox ([#48](https://github.com/AI-Substrate/flowspace3/issues/48)) ([f8bd87b](https://github.com/AI-Substrate/flowspace3/commit/f8bd87be63de63355d8ae5dcda337143543e5fe6))
* add OpenRouter through OpenAI-compatible providers ([#58](https://github.com/AI-Substrate/flowspace3/issues/58)) ([4ba23dc](https://github.com/AI-Substrate/flowspace3/commit/4ba23dc5b306b924aeddf8b6e91d59ebbceda9d2))
* **ask:** flowspace3 ask — a bounded, grounded agent loop over the index ([#45](https://github.com/AI-Substrate/flowspace3/issues/45)) ([e8d0a88](https://github.com/AI-Substrate/flowspace3/commit/e8d0a8864e3c13b08b947385ca7d6a28bfc14d41))
* **ask:** render for humans, record what the model saw, and make the budget real ([#57](https://github.com/AI-Substrate/flowspace3/issues/57)) ([4a8ad05](https://github.com/AI-Substrate/flowspace3/commit/4a8ad056cc612a2e4046d01eb55f3f9b776eb166))
* authenticate daemon HTTP requests ([#43](https://github.com/AI-Substrate/flowspace3/issues/43)) ([17d7708](https://github.com/AI-Substrate/flowspace3/commit/17d77086fb4a25d61915a56cc367ef1db7b1500a))
* **convo-ingest:** read agent conversations out of their native session stores (plan 005) ([#42](https://github.com/AI-Substrate/flowspace3/issues/42)) ([85cd3ad](https://github.com/AI-Substrate/flowspace3/commit/85cd3adaae288dd2423978c2c7800063ab69c05d))
* **daemon:** worktree lifecycle detection and checkout-scoped search ([#50](https://github.com/AI-Substrate/flowspace3/issues/50)) ([c8f5006](https://github.com/AI-Substrate/flowspace3/commit/c8f50065e0df0e6ff74f9b6291e3ed3babbd7177))
* **ddocs:** index deterministic documents as addressable rows ([#65](https://github.com/AI-Substrate/flowspace3/issues/65)) ([8cef085](https://github.com/AI-Substrate/flowspace3/commit/8cef0855827915bc377a67fd37363533717fc023))
* **harness:** team extension — worktree + next-ordinal plan scaffold ([#37](https://github.com/AI-Substrate/flowspace3/issues/37)) ([1d3bda9](https://github.com/AI-Substrate/flowspace3/commit/1d3bda904d0dd803245b807e9853e48c8848dd7c))
* human-first output, the tui verb, and the daemon event stream (plan 007) ([#52](https://github.com/AI-Substrate/flowspace3/issues/52)) ([37efe03](https://github.com/AI-Substrate/flowspace3/commit/37efe03105c32f2afe2c6ad289766aaeb49aef6a))
* **pij-team:** team-delivery skill prototype — tenets, dd schemas, packet templates, government settings ([#35](https://github.com/AI-Substrate/flowspace3/issues/35)) ([4a3f1d9](https://github.com/AI-Substrate/flowspace3/commit/4a3f1d9cf34cca0a4c49eca44ac7b5a886b358ab))
* **team:** harness team tidy — the teardown counterpart to team new ([#40](https://github.com/AI-Substrate/flowspace3/issues/40)) ([bb10474](https://github.com/AI-Substrate/flowspace3/commit/bb10474aa69594d72d5c34070c20d4ed21db7d29))


### Bug Fixes

* **ask:** refuse when the agent port cannot answer, instead of publishing a placeholder ([#55](https://github.com/AI-Substrate/flowspace3/issues/55)) ([8822ebf](https://github.com/AI-Substrate/flowspace3/commit/8822ebf4d9c92588feb43d244b02ecf7e4aae823))
* exit quietly on broken pipes ([#47](https://github.com/AI-Substrate/flowspace3/issues/47)) ([086f812](https://github.com/AI-Substrate/flowspace3/commit/086f812de58776a985f02f8951e4d0a11869d015))
* isolate conversation ingest lane ([#56](https://github.com/AI-Substrate/flowspace3/issues/56)) ([b2f9c65](https://github.com/AI-Substrate/flowspace3/commit/b2f9c65dd28d7411ba77eed926ad23ea05bfd083))
* make daemon boot and shutdown race-safe ([#64](https://github.com/AI-Substrate/flowspace3/issues/64)) ([c5fa703](https://github.com/AI-Substrate/flowspace3/commit/c5fa703fcd84a89a4747ceacce210887b8616a81))
* prevent queue starvation with lifo claims ([#61](https://github.com/AI-Substrate/flowspace3/issues/61)) ([bdb2dbb](https://github.com/AI-Substrate/flowspace3/commit/bdb2dbb1ebb2268f936d901fca453ce45d1c98b4))
* **query:** stop a scoped search being starved by the rest of the index ([#44](https://github.com/AI-Substrate/flowspace3/issues/44)) ([1325394](https://github.com/AI-Substrate/flowspace3/commit/1325394ed333e2ef796264f4f6817fc77501e483))
* reindex blobs after parser version changes ([#63](https://github.com/AI-Substrate/flowspace3/issues/63)) ([8aad313](https://github.com/AI-Substrate/flowspace3/commit/8aad313ab1f8709ab4471ceef0baf746859878ef))
* **team:** tidy must treat squash-merged branches as merged ([#41](https://github.com/AI-Substrate/flowspace3/issues/41)) ([ed188d4](https://github.com/AI-Substrate/flowspace3/commit/ed188d43675fb81f648cfa14b277300904713e39))
* **watcher,enrich:** stop indexing gitignored trees, and stop paying to embed them ([#38](https://github.com/AI-Substrate/flowspace3/issues/38)) ([037fd4e](https://github.com/AI-Substrate/flowspace3/commit/037fd4e5feff2370ab7e3c31caf7bb447ca6f896))


### Performance Improvements

* **daemon:** newly discovered worktree scans claim ahead of backlog ([#59](https://github.com/AI-Substrate/flowspace3/issues/59)) ([8817b1f](https://github.com/AI-Substrate/flowspace3/commit/8817b1f970d3220ad7b899e296c31ce4eaf481cd))

## [0.4.0](https://github.com/AI-Substrate/flowspace3/compare/v0.3.2...v0.4.0) (2026-08-27)


### Features

* **daemon:** conversations v1 phase 2 — intake endpoint, enrichment, and a gc fix ([#31](https://github.com/AI-Substrate/flowspace3/issues/31)) ([3bf19d1](https://github.com/AI-Substrate/flowspace3/commit/3bf19d1183612b573bff43eae51c76c219102580))
* **query:** conversations v1 phase 3 — import, search, window, outline ([#33](https://github.com/AI-Substrate/flowspace3/issues/33)) ([1890d3c](https://github.com/AI-Substrate/flowspace3/commit/1890d3c32716fb47ea6ed7de49a5bcaf17e16ca8))
* **query:** get/tree read surface and D6 cwd scoping ([#29](https://github.com/AI-Substrate/flowspace3/issues/29)) ([ad6a6ac](https://github.com/AI-Substrate/flowspace3/commit/ad6a6ac74d3cf2badbadbc03c6fd51e0eb1b8ed3))
* **store:** conversations v1 phase 1 — tables, turn elements, second reference leg ([#30](https://github.com/AI-Substrate/flowspace3/issues/30)) ([7618de1](https://github.com/AI-Substrate/flowspace3/commit/7618de174d8dd48d0383300151f430b60fa5524a))


### Bug Fixes

* **query:** one spelling per address scheme, generic 501 code, get steers its warnings ([#34](https://github.com/AI-Substrate/flowspace3/issues/34)) ([fcf28fb](https://github.com/AI-Substrate/flowspace3/commit/fcf28fb912099de4082fdc610f9b2b0b90863ecf))
* **test:** no test may reach the production database ([#32](https://github.com/AI-Substrate/flowspace3/issues/32)) ([50455b6](https://github.com/AI-Substrate/flowspace3/commit/50455b641873dba17ccbb13cd61744230f76961f))
* update state is per-install-path, boot-fresh and disk-reconciled ([#27](https://github.com/AI-Substrate/flowspace3/issues/27)) ([b3a5889](https://github.com/AI-Substrate/flowspace3/commit/b3a5889a35f7ac1a264561eaf86a63dad9bb1596))

## [0.3.2](https://github.com/AI-Substrate/flowspace3/compare/v0.3.1...v0.3.2) (2026-08-27)


### Bug Fixes

* oversized inputs must embed, not fail forever ([#24](https://github.com/AI-Substrate/flowspace3/issues/24)) ([76df138](https://github.com/AI-Substrate/flowspace3/commit/76df1388530b67e114b419250ff4c65f4183be48))

## [0.3.1](https://github.com/AI-Substrate/flowspace3/compare/v0.3.0...v0.3.1) (2026-08-27)


### Bug Fixes

* **release:** draft-first releases so latest never resolves to an assetless release ([#22](https://github.com/AI-Substrate/flowspace3/issues/22)) ([c5a86a6](https://github.com/AI-Substrate/flowspace3/commit/c5a86a61791a137d829a02bdaa9d581772a5f99c))

## [0.3.0](https://github.com/AI-Substrate/flowspace3/compare/v0.2.0...v0.3.0) (2026-08-27)


### Features

* daemon auto-update + user messages queue + config reference (req-0054/0058/0059) ([#13](https://github.com/AI-Substrate/flowspace3/issues/13)) ([9905ffe](https://github.com/AI-Substrate/flowspace3/commit/9905ffe1001f5e37333cb1e690470a23ec2b1dd2))
* **daemon:** rolling log files with real rotation, ANSI discipline, and panic capture ([#20](https://github.com/AI-Substrate/flowspace3/issues/20)) ([8f6eff2](https://github.com/AI-Substrate/flowspace3/commit/8f6eff23efa53779f3628b6423c9a21d63927e5c))
* **remove,gc:** the remove verb, mid-scan-safe, with a three-level garbage collector (req-0057) ([#19](https://github.com/AI-Substrate/flowspace3/issues/19)) ([99d0010](https://github.com/AI-Substrate/flowspace3/commit/99d00103db22ed6b7ede4da4feea2fc9cf6d8c28))


### Bug Fixes

* **deps:** commit the Cargo.lock the auto-update packet needed, and gate it ([#15](https://github.com/AI-Substrate/flowspace3/issues/15)) ([67e9a2e](https://github.com/AI-Substrate/flowspace3/commit/67e9a2e9721649a77237a7f3e82b00174dd277e3))
* **install:** windows script refuses loudly with WSL2 guidance instead of 404ing on an unpublished target ([1ae1427](https://github.com/AI-Substrate/flowspace3/commit/1ae142707dbf3502971052c54c246e51a6655a3d))
* **release:** stitch the version through to the binary, and guard the update loop (req-0060) ([#16](https://github.com/AI-Substrate/flowspace3/issues/16)) ([f6cf582](https://github.com/AI-Substrate/flowspace3/commit/f6cf5824e5f72cd1ac51313fa34b083e2fdb0f04))
* **schema:** name the binary-older-than-database case, and stop tests choosing production (req-0061) ([#18](https://github.com/AI-Substrate/flowspace3/issues/18)) ([f0707c8](https://github.com/AI-Substrate/flowspace3/commit/f0707c8d476552e30613ebceb08457d4b946b592))

## [0.2.0](https://github.com/AI-Substrate/flowspace3/compare/v0.1.0...v0.2.0) (2026-08-26)


### Features

* **cli:** req-0053 bundled agent skill + doctor install-skill ([546f475](https://github.com/AI-Substrate/flowspace3/commit/546f475693b445956d4c478f3f4b9bd8891b4c65))
* **cli:** req-0053 doctor skills row ([22485cc](https://github.com/AI-Substrate/flowspace3/commit/22485cc275693724087960bf3f6597b47861ae83))
* **cli:** req-0055 agents-start-here verb ([e4533fc](https://github.com/AI-Substrate/flowspace3/commit/e4533fc6c0ca03d106d4e4adfba1b3fe137b9d6c))
* **daemon:** live watcher — roots become watchers, edits become scan jobs ([c41c6c6](https://github.com/AI-Substrate/flowspace3/commit/c41c6c62729cc45ec3b8aebf42086cbbe25468f8))
* **daemon:** pure watcher core — debounce, max-age settle, directory keying ([3b83ec9](https://github.com/AI-Substrate/flowspace3/commit/3b83ec9bbdfc685ef51fdf5a15b2bc7a56470fe3))
* **daemon:** reconcile substrate, the doctrine's mechanics written once ([4d00f5b](https://github.com/AI-Substrate/flowspace3/commit/4d00f5b4df6b89111cf3e6b7eefb118e13cb42eb))
* **discovery:** case-insensitive deny list + shared cross-filter fixture ([2f78b7f](https://github.com/AI-Substrate/flowspace3/commit/2f78b7fa151d0d1dc84989218744cccd43a8fe26))
* **discovery:** standard deny list — node_modules and kin refused without a .gitignore ([74cecf7](https://github.com/AI-Substrate/flowspace3/commit/74cecf7f35134937746863ee913f9fbfcfdc0420))
* fs3 foundations — 7-crate workspace, two ports, composition root, drift check ([b812d4d](https://github.com/AI-Substrate/flowspace3/commit/b812d4dbe46ed17596d7b3de92c3ec3291bb128b))
* **install:** curl installer + ps1 twin; README install section, CI badge, ci-release service page ([067ba64](https://github.com/AI-Substrate/flowspace3/commit/067ba64142f1c14990c7d104adac685696ddc7f9))
* **release:** release-please rolling PR + cross-platform release builds (7 targets via plan-002 container, mac native build-only) ([26bb926](https://github.com/AI-Substrate/flowspace3/commit/26bb9261c5990212a481f41e2fbca84da801d731))
* **release:** single-binary matrix per req 51 - flowspace3 via fs3-cli; musl/windows-gnu dropped (ort-sys has no prebuilt, evidence in-file); mac jobs built-AND-run; installer picks linux-gnu; agent-first README ([f9cdcc2](https://github.com/AI-Substrate/flowspace3/commit/f9cdcc2ff183d5144b5a4f742939facffe57f1a0))


### Bug Fixes

* assert the promise the doc comment made — exactness within one response ([d93efa3](https://github.com/AI-Substrate/flowspace3/commit/d93efa3f2847290f7d86b46070e78761c365b5d9))
* **ci:** mirror compose port mapping 5433 so shipped daemon defaults hold on the runner ([a1e9de9](https://github.com/AI-Substrate/flowspace3/commit/a1e9de9b3fff76e8d9b3db156822e3060000bf86))
* **ci:** observe postgres readiness manually (image has no healthcheck) ([dcd09a5](https://github.com/AI-Substrate/flowspace3/commit/dcd09a5e18fc3e7847eacb60b4e113568ac08381))
* **daemon:** make the ignored-directories watcher test honest and deterministic ([894eca5](https://github.com/AI-Substrate/flowspace3/commit/894eca57408a4edf59e1f33c54765a5f477bba00))
* **daemon:** record the walked directory's blobs, ending the forever-rescan ([de7ae1b](https://github.com/AI-Substrate/flowspace3/commit/de7ae1b5bdbb806bc0622c8ff12052addb884c24))
* **hooks:** skip_children so a pushed module root is not rejected for a sibling in-flight file ([f81c792](https://github.com/AI-Substrate/flowspace3/commit/f81c792da2a03657bf8b0c948d3e7b536fb080a5))
* **readme:** Docker prerequisite stated; dd note - installer mechanics proven, published-Release check pending merge ([f3f3462](https://github.com/AI-Substrate/flowspace3/commit/f3f3462215772c09eeedceba7ff5b7feddd793de))
* **release:** use simple release type - root manifest is a virtual workspace ([edca0d0](https://github.com/AI-Substrate/flowspace3/commit/edca0d0f9736d5ed4b0bf42fb114cec842f33104))
* rev-0002 findings — redact keys, enforce loopback, prove the guards ([afe721e](https://github.com/AI-Substrate/flowspace3/commit/afe721ec6dac4edcabb146cf63b3aeddcfdb68bd))
* rev-0004 findings — a contract a real provider can satisfy, and a kind-aware graph ([8a500d8](https://github.com/AI-Substrate/flowspace3/commit/8a500d8d42d349b44d8de40678a9b9f9a96a22df))
* **testkit:** req-0053 allow sha2 for fs3-cli in the arch allow-list ([21403af](https://github.com/AI-Substrate/flowspace3/commit/21403afad41994044da8d380de37646246ac019a))
* unsweep three sibling lines my scoped add caught mid-flight ([17878b6](https://github.com/AI-Substrate/flowspace3/commit/17878b6a4c20297d08d35cddd995a74f22404679))

# Lightroom current-effect trial bridge

The user chose current Lightroom edits for artistic judgment and original native pixels for focus/shake. The Lua plugin delegates current rendering to Lightroom. It does not pair files, select photos, score images, write XMP, change develop settings, reimport exports or maintain a second review database. Rust verifies materials through the existing imaging/integrity foundations. Review policy remains held until current renditions are linked to a frozen job and an actual reviewer; saved artifacts alone do not unlock judgments.

## One-time installation and first test

Add the entire ImgCull.lrplugin folder in Lightroom Classic: File → Plug-in Manager → Add. Keep that folder in place. This is a trial bridge, not an Adobe-tested/signed extension. Syntax, mocked SDK workflow and cross-language tests cannot certify real Lightroom rendering.

Select 1–3 representative SDR photographs, preferably including a visible crop and color edit. Choose File → Plug-in Extras → 导出所选照片供选优（试运行）. Choose an output parent outside the source photo library. The plugin creates a new .imgcull-lr-* subdirectory and renders sequentially in the background. No per-photo dialogs; completion/error uses a fading bezel rather than a modal popup. Cancel from Lightroom's progress area. 查看最近导出状态 is an explicitly invoked status dialog with the output path and any error. Initial selections are frozen; subsequent UI selection changes do not change the export list. An empty selection cannot mean the entire filmstrip.

Trial limits: 1–10 images, supported SDR develop settings, online originals and files <=512MiB. Full current crop, JPEG quality 1, sRGB, no resizing, output sharpening or watermark. Cropping legitimately produces fewer pixels than the original; this does not lower the separate review job's native source floor. Lightroom continues rendering in the background but uses CPU, RAM and disk. Throughput has not been measured on the user's camera/library.

No upload/API request, shell execution or custom crypto in the plugin. SDK Unicode file reads support Chinese source paths. Small JSON control files are written to a temporary file, closed, then moved through the SDK to the Unicode output path. If the OS temporary path cannot be opened by Lua, fail explicitly; no shell/encoding-loss fallback. SDK moves refuse existing destinations. Temp files and output names use documented chooseUniqueFileName; no undocumented LrUUID dependency. The cooperating single-plugin runtime flag resets on plugin restart; it is not review progress. Partial output is kept for diagnosis and is never considered complete without manifest.json published last. This is not an fsync/power-loss or hostile-filesystem guarantee.

## Contract and validation

manifest.json schema1, exporter imgcull-lr-v1 contains catalog path/version and one ordered image record per LR identity. Each record contains persistent UUID, catalog-local ID, virtual-copy flag, physical source and SHA256, canonical reported develop-settings SHA256, edit time/count, reported cropped width/height, generated render-NNNN.jpg filename and full-file SHA256. Settings, reported edit state and original hashes are checked before/after rendering and again for earlier frames before publishing the whole batch. No full develop/mask recipe or repeated conversation history in the manifest.

The SDK develop table is experimental and may omit or change state; edit counts are not absolute counters. These checks detect reported changes, not every possible hidden Adobe/AI state transition. Do not edit photos during the trial export. The saved rendition is a snapshot, not proof that the live catalog remains unchanged indefinitely. HDR is rejected rather than silently converting a current HDR appearance into SDR advice. AI masks/generative edits, camera-specific behavior and color fidelity require the real LR test. No ICC/calibrated-color verification is claimed by the Rust verifier.

```text
imgcull verify-lightroom-export EXPORT_DIRECTORY
```

Rust rejects unpublished/malformed manifests, unsafe filenames, duplicate LR UUIDs, stale sources, modified render bytes, unknown fields and full-crop dimension mismatches (one-pixel rounding/orientation swap permitted). It fully decodes each rendition via imaging::NativePhoto, without implementing a second decoder. Compact output always has can_apply=false, contains_visual_verdict=false and live_catalog_verified=false. It does not open the catalog, establish all RAW/JPEG/XMP job bindings or infer correct artistic/technical judgments.

Virtual copies stay distinct. The plugin never deduplicates them by physical path; later job binding must resolve multiple LR variants sharing one physical XMP before metadata planning. Passing this verifier alone cannot remove policy's lightroom_current_appearance_unavailable gate.

## Maintenance

vendor/json.lua is upstream rxi/json.lua commit dbf4b2dd2eb7c23be2773c89eb059dadd6436f94, unmodified with its MIT license. Codec only canonicalizes table ordering for stable settings fingerprints; JSON scalar escaping/encoding uses the pinned library. Source: https://github.com/rxi/json.lua/tree/dbf4b2dd2eb7c23be2773c89eb059dadd6436f94

Primary Adobe API reference (mirror): https://lrc.mcor.dev/modules/LrExportSession.html and LrPhoto/LrDigest/LrFileUtils. Official SDK entry: https://developer.adobe.com/lightroom-classic/ . The plugin uses documented namespaces; do not invent SDK writeFile/UUID calls. SDK mocks use Lua5.1 and do not reproduce Lightroom's image renderer. CI passes actual Lua-generated synthetic JSON/JPEG artifacts into the Rust verifier; no private photographs/catalogs enter GitHub.

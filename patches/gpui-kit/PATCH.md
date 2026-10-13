# GPUI Kit patch

This is GPUI Kit 0.7.3 from crates.io with one Linux-only manifest change:
remove `x11` from the `gpui-pre-platform` and optional `gpui-fast-platform`
feature lists. Wayland and all upstream GPUI/GPUI Kit source remain unchanged.
This keeps every TaskForest GPUI platform path Wayland-only.

+++
title = "Network"
description = "What this site publishes for DreamWeave clients, indexes and mirrors, and whether each project is ready."
template = "dreamweave/network.html"

[extra]
comments = false
+++

Every page on this site links to `dreamweave.json`, the discovery index. The index names each
project's stable id and its manifest. A manifest lists releases, the archives in them, their
SHA-256 digests and where to download them. None of it needs JavaScript, an API or a
DreamWeave server; it is static files next to the pages you are reading.

The checks below are computed when the site is built. They cover structure, not taste.

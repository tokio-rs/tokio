# Tokio architecture

This document provides an in depth guide of the internals of Tokio, the various
components, and how they fit together. It expects that the reader already has a
fairly good understanding of how to use Tokio. A [getting started][guide] guide
is provided on the website.

[guide]: https://tokio.rs/docs/getting-started/hello-world

## Contents

<!-- TODO: Fill out as sections are written -->

* [Runtime model](runtime-model.md) - An overview of Tokio's asynchronous
  runtime model.
* [Non-blocking I/O](net.md) - Implementation details of Tokio's network related
  types (TCP, UDP, ...).

<!--
## Overview

  * Tasks
  * Executors
  * Resources
  * Drivers

&mut https://github.com/tokio-rs/tokio/issues/615
-->

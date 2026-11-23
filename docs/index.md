---
layout: home

hero:
  name: idbseq
  text: Unique, increasing IDs in one portable file
  tagline: >-
    Embed it in your app like SQLite: named sequences, no server, works offline,
    and never the same ID twice, even across crashes. When several machines
    need one sequence, serve the same file over HTTP.
  image:
    light: /logo.svg
    dark: /logo-dark.svg
    alt: idbseq
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: What is idbseq?
      link: /guide/introduction
    - theme: alt
      text: Run a server
      link: /guide/server

features:
  - title: No server
    details: >-
      A library for Rust and Node.js. It runs inside your desktop app, CLI tool
      or service, offline, with nothing else to install or run.
  - title: One portable file
    details: >-
      The same format on every OS. Copy it, back it up, share it between
      processes on one machine, or hand it to a server later.
  - title: Never a duplicate
    details: >-
      Every ID is on disk before it is returned. After a crash or a restart, the
      next ID is always greater than any ID handed out before.
  - title: Many independent sequences
    details: >-
      Create a collection per thing you number (orders, invoices, tickets), each
      with its own start, step and maximum.
  - title: Fast when you need it
    details: >-
      Calls made at the same time share one disk write, batches return thousands
      of IDs per call, and block mode serves from memory between reservations.
  - title: A server when you outgrow one machine
    details: >-
      idbseqd serves the same engine over HTTP/JSON to any language, with scoped
      API keys, an admin console, an audit log, TLS and an MCP server for AI agents.
---

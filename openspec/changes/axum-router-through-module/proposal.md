# Proposal: Read an axum Router imported through its module

## Why

axum exports `Router` from `axum` and from `axum::routing`. A type that a `use` of another crate brings in is read whole, so `use axum::routing::{get, Router};` with `router.route("/x", get(show))` on a parameter or a field of type `Router` serves nothing, while `use axum::{routing::get, Router};` serves the route. The `reqwest` pack lists `reqwest.blocking.Client.new` for the same reason.

## What Changes

- The `axum` pack lists `axum.routing.Router.new` and `axum.routing.Router.default` in the receiver of its `route` rule.
- A `Router` that a `use` brings in from another crate stays no router of the pack, also when its module is called `routing`.

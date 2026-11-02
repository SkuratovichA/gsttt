# Proposal: A literal glued to a placeholder before `//` is no origin

## Why

The property test `a_literal_glued_to_a_base_url_is_the_first_segment` failed on some seeds. `${x}0//{}` was read as the protocol `${x}0`, `//` and the host `{}`, so its path was lost and it became `/`, where `/0//{}` becomes `/0/{}`. The requirement says a placeholder, `//` and a host that holds a placeholder are an origin: the protocol has to be a placeholder, but the code only checked that it holds one. `${base}api//${id}` was lost the same way.

## What Changes

- In `path_after_origin`, the protocol before a bare `//` must be made of placeholders alone, as `${}`, `${a}${b}` or `{scheme}`. A literal glued to the placeholder makes the text a base URL that starts a path, so `${base}api//${id}` becomes `/api/{}` and `${x}0//{}` becomes `/0/{}`. The protocol before `://` is unchanged, so `http${s}://h/x` is still an origin, and so are `${}//${}/api/x` and the rewritten `${}//api/x`.
- The requirement "The origin of a URL" says "Placeholders alone" and gets a scenario for the glued literal.
- `FACT_FORMAT` goes from 11 to 12, because the core now makes other facts from the same text.

## Impact

- Capability `contract-formats`.
- `crates/core/src/contract.rs`, `crates/core/src/engine/keys.rs`.
- The version bump reads every file again once.

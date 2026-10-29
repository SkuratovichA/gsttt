# Design

## Context

The MCP server returns the text the command line prints, with lists cut at 30 entries unless `all` is set (`crates/mcp/src/ask.rs`). `all` lifts the cap whatever the size, and a list of a large repository has hundreds of entries.

## The number

An answer holds at most 25,000 characters, counted as characters and not as bytes. That is half of the 50,000 characters above which Claude Code saves a result to a file, and under its warning at 10,000 tokens.

## First(n) in core

The cutter is `Entries::First(n)` in `gsttt-core`, not a second one in the MCP crate. Every answer already writes its lists through `rows` and its names through `name_list`, so a count of rows is one more case of the cut those two make. A cutter in the MCP crate would work on text, would have to know what a row and a count line are, and would drift from the command line. `First(n)` cuts the names of a line as `Capped` does, three and a count, because in an answer cut for its size the rows are what an agent needs and the names in a reason are not, so `First(LIST_CAP)` writes exactly what `Capped` writes. The problems of a coverage answer stay uncut, and a coverage answer whose problems alone pass 25,000 characters goes out as it is, after the cut line.

## The halving

`within_budget` returns the text with the entries asked for when it fits. Otherwise it looks for the `n` for which the cut line, a newline and the text with `First(n)` fit, and `fits(n)` says that. `upper` is the most that can matter: `LIST_CAP` for `Capped`, `n` for `First(n)`, and the length of the uncut text for `All`, because every entry takes a character. If `fits(upper)`, that is the answer. If `fits(0)` is false, nothing can be shortened and the text with `First(0)` goes out. Otherwise `low = 0` fits and `high = upper` does not, and halving until `high == low + 1` leaves `n = low`, where `n` fits and `n + 1` does not. A larger `n` can give a text a few characters shorter, when a `… N more` line goes away, so the halving finds a boundary and not always the largest `n`; it finds the same one for the same answer.

## The cut line comes first

An agent reads the start of a result first, and a result that is partial must say so before the part it holds. The line names the limit and says that the counts say what the lists leave out. Its number is written with `thousands(BUDGET)`, and the descriptions of `all` in the tool listing are tested against the same constant, so the three cannot drift.

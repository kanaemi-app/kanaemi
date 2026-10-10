# Kanaemi

どの OS でも同じ打ち方の日本語入力

## How it is built

- **Test first.** Red → green → refactor. Write the failing test before the
  code that makes it pass.
- **Each layer holds one thing.** Put a fact in the one layer that owns it,
  and nowhere else.

  | Layer | Holds |
  | --- | --- |
  | `docs/concept.md` | what this is and for whom |
  | `docs/adr/` | one decision and why it was made |
  | `docs/spec/` | the behaviour users and other components can rely on |
  | Tests | the details that pin the behaviour down: rules, boundaries, tables |
  | Code | how it is done |
  | Commits | why a change was made |
  | Comments | why it is not done another way |

  Before writing, fixing or reviewing a record, read `docs/adr/README.md`;
  before writing, fixing or reviewing a spec, read `docs/spec/README.md`.
  Follow their writing order.
- **Dependencies are current.** Before adding or bumping one, look up its
  latest release in the registry. Do not copy a version from memory or from
  another repository.

## Quality gate

Run `just ci` before reporting a change as done. CI runs the same recipe.

## Layout

| Path | What lives there |
| --- | --- |
| `apps/` | the input method for each OS (`macos`, `windows`, `ibus`) and the settings app (`settings`) |
| `crates/` | libraries: `core`, `engine`, `config`, `runtime`, `functions`, `bench-support` |
| `docs/` | concept, ADRs, specs, references |
| `.github/` | CI, packaging and release workflows, and the scripts they run |
| `.cargo/` | Cargo configuration for the build |

## Language

- README, docs and commit messages: Japanese
- Code comments, log and error messages: English

## Commits

Scoped Commits, from now on; earlier history stays as it is:

```
<scope>: <description>

<why this change is needed>
```

- The scope names where the change is, from the table below. Several scopes
  are joined with `, `; a change across the whole tree uses `treewide`.
- The scope is English; the description and the body are Japanese. The
  description has no trailing period and stays within 72 characters.
- The body explains why. The code shows how; the tests show what.
- A release commit is `release: vX.Y.Z`.
- A breaking change says so in the body and carries a `BREAKING CHANGE:`
  trailer.

| Scope | Covers |
| --- | --- |
| `core` | `crates/core/` |
| `engine` | `crates/engine/` |
| `config` | `crates/config/` |
| `runtime` | `crates/runtime/` |
| `functions` | `crates/functions/` |
| `bench` | `crates/bench-support/` and the benchmarks |
| `macos` | `apps/macos/` |
| `windows` | `apps/windows/` |
| `ibus` | `apps/ibus/` |
| `settings` | `apps/settings/` |
| `adr` | `docs/adr/` |
| `spec` | `docs/spec/` |
| `docs` | other docs: `docs/concept.md`, `docs/references/`, `README.md` |
| `ci` | `.github/` |
| `release` | the release and packaging |
| `nix` | `flake.nix`, `flake.lock` |
| `deps` | dependency updates |
| `treewide` | a change across the whole tree |

Add a row when a new area appears.

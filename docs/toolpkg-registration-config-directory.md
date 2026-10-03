# Configuration directories during ToolPkg registration

`ToolPkg.getConfigDir()` remains available at module top level, inside
`registerToolPkg()`, and during normal runtime execution. A registration-stage API
ban is not a valid fix for a lock dependency.

## Root cause

Package scanning executes registration JavaScript while its caller owns the
package-manager mutex. The scope-aware configuration callback used to acquire
that same mutex to resolve the owner, forming a circular wait. In addition, a
first import cannot look up an installation record that is committed only after
registration succeeds.

## Registration context

Archive loading selects `ToolPkgConfigScope` before JavaScript evaluation:

- Built-in and bundled candidate archives use the explicit device scope.
- Newly imported external/market archives use their selected device destination.
- Scanning an installed archive derives its scope by matching the exact host-backed
  device or space package directory; an unrelated path is a validation error.

The main-registration parser supplies the manifest package ID and selected scope
as internal call parameters. The JavaScript engine binds a Rust-owned snapshot
for the registration operation, before evaluating the main module. A configuration
request invokes `registration_plugin_config_dir()` with that snapshot, not the
runtime installation catalog. Named directories remain inside the owner's
`namespaces` directory. The context is cleared when registration returns,
including error returns.

The callback creates the directory through the existing filesystem Host and
returns an absolute VFS path. It never acquires the package-manager mutex and
never creates a phantom installation record. Missing or invalid internal scope,
owner mismatch, and filesystem failures are reported as errors rather than
choosing a different storage location.

## Runtime context

Normal execution resolves container/subpackage ownership from `ExtensionStore`
and creates the same stable scope-owned directory. Registration and runtime path
construction share `configPathForScope()` and the same filesystem Host helper.
The structured host-result decoder continues to throw failures; it must not
return an error JSON document as a path string.

## Regression coverage

- The production SDK JavaScript is exercised by
  `plugins/tools/plugin_loading.test.mjs`: top-level directory access, named
  directories inside registration, and actual host-error propagation.
- Rust engine tests hold the simulated loading-manager lock while registration
  queries device and space paths. They also cover the explicit scope requirement.
- Store tests verify first-import paths without installation records and stable
  paths after registration.

Rust unit tests require a subsequent Rust test run; syntax checks alone do not
establish that the native execution path has been exercised.

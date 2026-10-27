# Release process

Initial version: `rjupa-mcp@0.1.0`. No release is assumed to exist yet.
The package's release repository is **sindrig/family-mcp**, not upstream.
Tags use `rjupa-mcp@<version>`; use default features only, never `test-origin`.

From the root after the Rust checks in the package README:

```sh
bun run --cwd packages/rjupa-mcp release:sync
bunx turbo run release:check --filter=rjupa-mcp
bunx turbo run build:binary test:binary test:installer --filter=rjupa-mcp --force
```

The shared installer, archives, checksums and dependency notices follow the other
native MCPs. Release tooling defaults existing packages to their existing upstream
repository; only this package explicitly selects the fork. Existing packages'
generated installers are unchanged. Do not publish or tag without maintainer
authorization. A pull request is not a published release or an installed MCP.

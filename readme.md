# multitool

Companion CLI for working with [rules_multitool](https://github.com/theoremlp/rules_multitool) lockfiles.

## Updating Dependencies

multitool supports a simple and somewhat naive update strategy for GitHub release artifacts using the `update` command.

```sh
multitool --lockfile ./multitool.lock.json update
```

## Example for `multitool.lock.json`

```json
  "multitool": {
    "binaries": [
      {
        "kind": "archive",
        "url": "https://github.com/theoremlp/multitool/releases/download/v0.9.0/multitool-aarch64-unknown-linux-gnu.tar.xz",
        "file": "multitool-aarch64-unknown-linux-gnu/multitool",
        "sha256": "693b66def2d8dacdfcb5a011b7c32a8e89a13bdc031db8dc40c0759a38253103",
        "os": "linux",
        "cpu": "arm64"
      },
      {
        "kind": "archive",
        "url": "https://github.com/theoremlp/multitool/releases/download/v0.9.0/multitool-x86_64-unknown-linux-gnu.tar.xz",
        "file": "multitool-x86_64-unknown-linux-gnu/multitool",
        "sha256": "34dc5968dad458a4050ebde58e484ebb7c624a119e4031347683eeb80922e6df",
        "os": "linux",
        "cpu": "x86_64"
      },
      {
        "kind": "archive",
        "url": "https://github.com/theoremlp/multitool/releases/download/v0.9.0/multitool-aarch64-apple-darwin.tar.xz",
        "file": "multitool-aarch64-apple-darwin/multitool",
        "sha256": "8f5e0cd033b0fcc128c762f8d527110433523da378619cecf40e91c1df5c0686",
        "os": "macos",
        "cpu": "arm64"
      },
      {
        "kind": "archive",
        "url": "https://github.com/theoremlp/multitool/releases/download/v0.9.0/multitool-x86_64-apple-darwin.tar.xz",
        "file": "multitool-x86_64-apple-darwin/multitool",
        "sha256": "abde3bbbd49a09d33048e1f55cc9a8aa5dd61a13493a0b1bd5ffc2c56e5bf837",
        "os": "macos",
        "cpu": "x86_64"
      }
    ]
  },
```

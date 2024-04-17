# multitool

Companion CLI for working with [rules_multitool](https://github.com/theoremlp/rules_multitool) lockfiles.

## Updating Dependencies

multitool supports a simple and somewhat naive update strategy for GitHub release artifacts using the `update` command.

```sh
multitool --lockfile ./multitool.lock.json update
```

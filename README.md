# gwab

`gwab` is a stateful HTTP command-line client. It currently supports GET,
POST, PUT, PATCH, and DELETE requests, custom headers and bodies, persistent
variables, `{{variable}}` interpolation, and relative URLs resolved against a
saved `base_url`. Add `--verbose` to print request/response headers and bodies,
DNS lookup timing, and total request latency. Verbose output includes header and
body values, so it can expose credentials.

Variables are stored as JSON in the platform's application data directory
(typically `~/.local/share/gwab/settings.json` on Linux, honoring
`XDG_DATA_HOME`). The JSON object is keyed by the absolute current working
directory, with each directory holding its own variable map. Existing flat
settings are treated as the current directory's values and rewritten in the
scoped format on the next change. On Unix, gwab restricts the directory to the
current user and the state file to owner read/write access. The file is still
plaintext;
do not store credentials there if your local threat model requires encrypted
storage. `gwab vars` redacts values for names containing `token`, `secret`,
`password`, `credential`, `api_key`, or `apikey`.

Configuration files add reusable defaults without replacing workspace state.
Values merge by key in this order (highest precedence first): request-line
`--var` overrides, the current directory's saved variables, the nearest
ancestor `gwab.toml`, the global config file, then built-in defaults. The global
file is `gwab/config.toml` under the platform config directory (typically
`~/.config/gwab/config.toml` on Linux, honoring `XDG_CONFIG_HOME`). Project
configuration is optional and can be checked into a repository; mutable state
and credentials remain in the external, directory-scoped JSON store.

Both config files use TOML. Variables can be written as top-level scalar keys
or grouped under `[vars]`:

```toml
base_url = "https://api.example.com"

[vars]
tenant = 1
api_version = "v2"
```

Named environments use `[env.NAME]` (or `[env.NAME.vars]`). Select one for a
request with `--env NAME`; its values override ordinary merged variables for
that request, while `--var` remains the final override:

```toml
[env.staging]
base_url = "https://staging.example.com"
tenant = 2

[env.production.vars]
base_url = "https://api.example.com"
tenant = 1
```

```sh
gwab get /posts --env staging
gwab get /posts --env staging --var tenant=3
```

Global and project definitions for the same environment merge by key, with
project values taking precedence. Selecting an undefined environment reports
an error instead of silently using ordinary configuration.

String, numeric, boolean, and datetime values are converted to strings. Arrays
and tables cannot be used as variable values. If a key appears more than once
in a file, `[vars]` or `[env.NAME.vars]` takes precedence over the matching
top-level key.

The current implementation does not yet persist cookies, support `--prompt`,
or resolve `:name` URL path parameters.

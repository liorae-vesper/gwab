# gwab

`gwab` is a stateful HTTP command-line client. It currently supports GET,
POST, PUT, PATCH, and DELETE requests, custom headers and bodies, persistent
variables, `{{variable}}` interpolation, and relative URLs resolved against a
saved `base_url`. Add `--verbose` to print request/response headers and bodies,
DNS lookup timing, and total request latency. Verbose output includes header and
body values, so it can expose credentials. Add `--quiet` to suppress output and
use the process exit code as the result: `0` for a 2xx response and `1` for any
other status or request error. `--quiet` also suppresses `--verbose` output.

If stdin is piped and `--body` is not set, gwab sends stdin as the request
body. Choose `--output body` to write only the raw response body bytes, or
`--output all` to write the response status line, headers, and body. The
default output remains a status line followed by a text body. Since response
output goes to stdout, normal shell pipes and redirects work:

```sh
cat my-body.json | gwab post /posts --add-header 'Content-Type: application/json'
gwab get /posts --output body | jq .
gwab get /posts --output body > response.json
gwab get /posts --verbose --output all > gwab.log 2>&1
```

Verbose request details and timing go to stderr; redirect both streams with
`2>&1` to keep them in the same log. `--body` takes precedence over piped
stdin.

Undefined `{{variables}}` are requested once in an interactive terminal and
reused for the current request; prompted values are not saved. Without an
interactive terminal, gwab fails before sending the request and suggests using
`--var` or a configuration file. `--quiet` suppresses prompting as well as
output.

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

Cookies persist separately in `cookies.json` in the same application data
directory. On Unix, the cookie file is restricted to owner read/write access.
Cookie values are plaintext on disk; protect or remove the file if they grant
access to sensitive accounts.

Configuration files add reusable defaults without replacing workspace state.
Values merge by key in this order (highest precedence first): request-line
`--var` overrides, the current directory's saved variables, the nearest
ancestor `gwab.toml`, the global config file, then built-in defaults. The global
file is `gwab/config.toml` under the platform config directory (typically
`~/.config/gwab/config.toml` on Linux, honoring `XDG_CONFIG_HOME`). Project
configuration is optional and can be checked into a repository; mutable state
and credentials remain in the external, directory-scoped JSON store.

### Saved requests

Save a request with `-s` or `--save` and replay it from the same directory with
`gwab run METHOD NAME`. Add `--description` when saving to label it in the
picker. Run `gwab run` without a method or name to fuzzy search all saved
requests; each row shows the method and name beside its description:

```sh
gwab get /users --add-header 'Accept: application/json' --env staging --save users --description 'List users'
gwab run get users
gwab run
```

The picker opens only when both stdin and stderr are connected to a terminal.
Type to filter requests, use the arrow keys to choose one, and press Enter to
run it. Press Esc or `q` to cancel without sending a request. In scripts or
other non-interactive contexts, provide the method and name explicitly.

Named requests live in `gwab-requests.toml` in the current directory, separate
from `gwab.toml`. A request saved as `users` for GET is stored under the TOML
table `["get users"]` with the URL, headers, body, variables, environment,
and output flags used on the command line. Piped stdin is captured as the body
when saving. Saving the same method and name updates that entry; names are
independent across HTTP methods. The file stores values as written, so use
variable interpolation such as `{{auth_token}}` instead of putting credentials
directly in saved headers or variables.

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

The current implementation does not yet support `--prompt` or resolve `:name`
URL path parameters.

# deimos

EVE Online's game data as JSON, without installing the game.

A fitting engine needs EVE's raw data: items, attributes, effects and their modifiers, skill
requirements, warfare buffs, mutaplasmid rolls, fighter abilities, space components. Getting all of
it has meant [Phobos](https://github.com/pyfa-org/Phobos), which reads an installed client, on
Windows, through the client's own Python loaders. Deimos reads CCP's published static data export
and the client's files on CCP's content CDN instead, and writes the same layout Phobos produces, so
an engine that reads a Phobos dump reads this one unchanged. It fills all fourteen collections
[refine](https://github.com/DarkFenX/refine) asks for. It is not a Phobos clone: of Phobos's 149
`fsd_built` containers it builds 9, and its pickle, bulkdata, sqlite and traits miners have no
counterpart here, because no fitting engine asks for them.

## Use it

Dumps are published as [releases](https://github.com/NicolasKion/deimos/releases), rebuilt whenever
CCP ships a new export or client build.

```sh
target/release/deimos ./eve-data                     # build one yourself
target/release/deimos ./eve-data --export ./sde.zip  # reusing an export you have
target/release/deimos --list                         # what the client has, and what is readable
```

```rust
use deimos::dump::{self, Containers, Tables};

let dump = dump::write_latest("./eve-data".as_ref(), None, Containers::All, Tables::All)?;
# Ok::<(), deimos::Error>(())
```

## Output

```text
<out>/phobos/metadata.json          build the data came from
<out>/fsd_lite/<container>.json     client .static containers, verbatim
<out>/fsd_built/<container>.json    export tables in the client's shape, and space components
<out>/sde/<table>.json              every export table, verbatim
```

Each file is a JSON object keyed by entity ID. `fsd_built` holds only containers whose client shape
is known and checked; the rest of the export lands in `sde` in its own shape rather than a guessed
one.

## Sources

The static data export is a zip of JSONL files CCP publishes for third parties.

The content CDN serves the client's own files, reached the way the launcher reaches them:
`eveclient_TQ.json` names the build, `eveonline_<build>.txt` indexes it, and the resource index maps
`res:/` paths to files on `resources.eveonline.com`. Client data comes in two formats. `.static`
files are SQLite databases of JSON, read directly; fighter abilities live there and the export has
no equivalent. `.fsdbinary` files are packed, and the only decoder is a Python 2.7 extension for
64-bit Windows shipped inside the client.

## Space components

`spacecomponentsbytype` is `.fsdbinary` with no counterpart in the export, which costs system-wide
effects, skyhook silo links, insurgency tackle range, Pochven subpylons and nexus event buffs. Wine
runs the client's decoder, which is what EVE's own macOS client does, that client being the Windows
build in a translation layer.

```sh
tools/space-components.sh ./eve-data     # 11,144 entries
```

Needs `wine`, `msitools`, and an amd64 host with 4 KB memory pages, so not Docker on Apple Silicon,
where Wine aborts against 16 KB pages. The release workflow does this on a runner.

## Tests

`cargo test` talks to CCP's servers, since that is what the crate does. One test also wants an
export, which is a 100 MB download:

```sh
DEIMOS_SDE_ZIP=/path/to/eve-online-static-data-latest-jsonl.zip cargo test
```

It checks warfare buffs derived from the export against the same records read out of the client's
own file, two separate routes to the same 276 entries.

## Licence

MIT. EVE Online and all related data are the property of CCP hf.

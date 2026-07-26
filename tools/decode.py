# -*- coding: utf-8 -*-
"""Decodes spacecomponentsbytype.fsdbinary using the client's own loader. Python 2.7, under Wine.

The loader is a CPython extension the client ships for exactly this file. It hands back objects
whose fields are ordinary attributes, so turning them into JSON is a recursive walk - the same shape
of work Phobos does with its EveNormalizer.

    python decode.py <loader.pyd> <data.fsdbinary> <out.json>
"""

import imp
import json
import os
import sys

PRIMITIVES = (bool, int, long, float, str, unicode, type(None))


def normalize(obj, loader):
    """Turn whatever the loader returns into JSON-able builtins."""
    if isinstance(obj, PRIMITIVES):
        # Non-ASCII in byte strings is cp1252 in EVE data.
        return obj.decode("cp1252") if isinstance(obj, str) else obj
    # Maps: FSD dictionaries and the top-level container itself.
    if hasattr(obj, "iteritems"):
        return dict(
            (normalize(key, loader), normalize(value, loader)) for key, value in obj.iteritems()
        )
    if isinstance(obj, (list, tuple, set, frozenset)):
        return [normalize(item, loader) for item in obj]
    # FSD objects, and the vector types FSD defines outside the loader module.
    if sys.modules.get(type(obj).__module__) is loader or type(obj).__name__.endswith("_vector"):
        skip = ("n_fields", "n_sequence_fields", "n_unnamed_fields")
        return dict(
            (name, normalize(getattr(obj, name), loader))
            for name in dir(obj)
            if not (name.startswith("__") and name.endswith("__")) and name not in skip
        )
    # Anything left that iterates, iterates.
    if hasattr(obj, "__iter__"):
        return [normalize(item, loader) for item in obj]
    raise TypeError("cannot normalize {0!r}".format(type(obj)))


def main(loader_path, data_path, out_path):
    # The loader has to be imported under its own module name, since that is what its init symbol
    # is called.
    name = os.path.splitext(os.path.basename(loader_path))[0]
    loader = imp.load_dynamic(name, loader_path)
    print("loaded {0}".format(name))

    data = loader.load(data_path)
    container = normalize(data, loader)
    print("decoded {0} entries".format(len(container)))

    # Keys come back as ints; JSON object keys are strings either way, which is what a dump wants.
    out_dir = os.path.dirname(out_path)
    if out_dir and not os.path.isdir(out_dir):
        os.makedirs(out_dir)
    with open(out_path, "wb") as handle:
        json.dump(container, handle, sort_keys=True)
    print("wrote {0}".format(out_path))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])

#!/usr/bin/env python3
"""Downloads the two client files the decoder needs, straight from CCP's CDN.

The route is the launcher's: eveclient_TQ.json names the build, eveonline_<build>.txt indexes the
application files, and the resource index it points at maps res:/ paths to content hashes.
"""

import json
import sys
import urllib.request

# The CDN turns away requests with no user agent of their own.
AGENT = "deimos/0.1 (+https://github.com/NicolasKion/deimos)"
BINARIES = "https://binaries.eveonline.com"
RESOURCES = "https://resources.eveonline.com"
LOADER = "app:/bin64/spaceComponentsByTypeLoader.pyd"
DATA = "res:/staticdata/spacecomponentsbytype.fsdbinary"


def open_url(url):
    return urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": AGENT}), timeout=300)


def get(url):
    with open_url(url) as response:
        return response.read()


def lookup(index_url, wanted):
    """Content hashes for the given paths, from one of the client's CSV index files."""
    found = {}
    with open_url(index_url) as response:
        for line in response:
            fields = line.decode("utf-8", "replace").split(",")
            if len(fields) > 1 and fields[0] in wanted:
                found[fields[0]] = fields[1]
                if len(found) == len(wanted):
                    break
    missing = set(wanted) - set(found)
    if missing:
        raise SystemExit("error: the client index has no {}".format(", ".join(sorted(missing))))
    return found


def main(out_dir):
    build = json.loads(get("{}/eveclient_TQ.json".format(BINARIES)))["build"]
    print("client build {}".format(build), flush=True)

    app = lookup("{}/eveonline_{}.txt".format(BINARIES, build), {LOADER, "app:/resfileindex.txt"})
    res = lookup("{}/{}".format(BINARIES, app["app:/resfileindex.txt"]), {DATA})

    for name, url in (
        ("spaceComponentsByTypeLoader.pyd", "{}/{}".format(BINARIES, app[LOADER])),
        ("spacecomponentsbytype.fsdbinary", "{}/{}".format(RESOURCES, res[DATA])),
    ):
        body = get(url)
        with open("{}/{}".format(out_dir, name), "wb") as handle:
            handle.write(body)
        print("  {} {} KiB".format(name, len(body) // 1024), flush=True)

    print(build)


if __name__ == "__main__":
    main(sys.argv[1])

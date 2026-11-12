"""Builds the shared brand sticker data from a Simple Icons release.

Usage:
    python scripts/build-brand-stickers.py <simple-icons.json> <iconify-simple-icons.json> <version>

<simple-icons.json> is `data/simple-icons.json` from the simple-icons npm
package (https://cdn.jsdelivr.net/npm/simple-icons@<version>/data/simple-icons.json);
<iconify-simple-icons.json> is https://api.iconify.design/collection?prefix=simple-icons
(the icons the stickers are drawn from). Writes
apps/web/src/stickers/providers/brands.json, read by the web app and the
desktop. The Simple Icons data is CC0-1.0.

Kept: icons Iconify serves whose own licence (when one is given) allows use
without share-alike or copyleft terms; CC-BY icons are kept and marked as
needing credit. Each brand is [slug, title, hex, guidelines, licence, credit].
"""

import json
import sys
from pathlib import Path

ALLOWED = {"", "CC0-1.0", "MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "MPL-2.0", "0BSD", "Unlicense"}
CREDIT = {"CC-BY-4.0", "CC-BY-3.0"}


def main() -> None:
    source, iconify, version = sys.argv[1], sys.argv[2], sys.argv[3]
    icons = json.loads(Path(source).read_text(encoding="utf-8"))
    collection = json.loads(Path(iconify).read_text(encoding="utf-8"))
    served = set(collection.get("uncategorized", []))
    for names in (collection.get("categories") or {}).values():
        served.update(names)
    brands = []
    dropped = {}
    for icon in icons:
        slug = icon["slug"]
        licence = (icon.get("license") or {}).get("type", "")
        if slug not in served:
            dropped["not on Iconify"] = dropped.get("not on Iconify", 0) + 1
            continue
        if licence not in ALLOWED and licence not in CREDIT:
            dropped[licence] = dropped.get(licence, 0) + 1
            continue
        brands.append([slug, icon["title"], icon["hex"].upper(), icon.get("guidelines", ""), licence, licence in CREDIT])
    brands.sort(key=lambda brand: brand[1].lower())
    out = {
        "source": f"simple-icons@{version}",
        "sourceUrl": "https://simpleicons.org",
        "license": "CC0-1.0",
        "trademark": (
            "Brand logos are trademarks of their owners. Use them only to refer to the brand, "
            "follow the brand's guidelines and do not suggest endorsement."
        ),
        "brands": brands,
    }
    target = Path(__file__).resolve().parent.parent / "apps" / "web" / "src" / "stickers" / "providers" / "brands.json"
    target.write_text(json.dumps(out, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    print(f"{len(brands)} brands written to {target}; dropped {dropped}")


if __name__ == "__main__":
    main()

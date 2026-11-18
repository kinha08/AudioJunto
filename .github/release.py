"""Publish artifacts built from this commit, leaving failed uploads in draft."""
import json
import os
import pathlib
import re
import urllib.error
import urllib.request

repo = os.environ["GITHUB_REPOSITORY"]
commit = os.environ["GITHUB_SHA"]
version = os.environ["RELEASE_VERSION"]
tag = f"v{version}"
base = f"https://api.github.com/repos/{repo}"
headers = {"Authorization": f"Bearer {os.environ['GH_TOKEN']}",
           "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"}

def api(url, method="GET", data=None, content_type="application/json"):
    body = json.dumps(data).encode() if isinstance(data, dict) else data
    req = urllib.request.Request(url, data=body, method=method,
                                 headers={**headers, "Content-Type": content_type})
    with urllib.request.urlopen(req, timeout=120) as response:
        return json.load(response)

try:
    release = api(f"{base}/releases/tags/{tag}")
except urllib.error.HTTPError as error:
    if error.code != 404:
        raise
    release = None

if release and not release["draft"]:
    print(f"Release {tag} is already published; existing assets were preserved.")
else:
    changelog = pathlib.Path("CHANGELOG.md").read_text(encoding="utf-8")
    section = re.search(rf"(?ms)^## {re.escape(version)}\s*\n(.*?)(?=^## |\Z)", changelog)
    changes = section.group(1).strip() if section else "See CHANGELOG.md for details."
    notes = f"""## AudioJunto {version}

Mirror Windows audio to two or more devices with a simple interface and system tray controls.

### Changes
{changes}

### Downloads
- **AudioJunto-Setup-{version}.exe**: per-user installer (recommended).
- **AudioJunto-{version}.exe**: portable application.
- **SHA256SUMS.txt**: download checksums.

Windows 10/11 x64. Interface and installer in Brazilian Portuguese. No virtual audio driver required.

### Português
Consulte o CHANGELOG para os detalhes desta versão.
Encerre a versão anterior antes de atualizar. Se ela estiver presa na bandeja, encerre o processo AudioJunto pelo Gerenciador de Tarefas.

Build: `{commit}`. Automated tests pass; listening and hardware-specific synchronization require testing on your devices.
"""
    if release is None:
        release = api(f"{base}/releases", "POST", {"tag_name": tag, "target_commitish": commit,
                      "name": f"AudioJunto {version}", "body": notes, "draft": True})
    elif release["target_commitish"] != commit:
        raise RuntimeError("Existing draft points to a different commit; refusing to replace it")
    for asset in release.get("assets", []):
        # A draft from a failed attempt can be retried without publishing duplicates.
        req = urllib.request.Request(f"{base}/releases/assets/{asset['id']}",
                                      method="DELETE", headers=headers)
        with urllib.request.urlopen(req, timeout=30):
            pass
    upload = release["upload_url"].split("{")[0]
    for name in (f"AudioJunto-Setup-{version}.exe", f"AudioJunto-{version}.exe", "SHA256SUMS.txt"):
        path = pathlib.Path("dist", name)
        api(f"{upload}?name={name}", "POST", path.read_bytes(), "application/octet-stream")
        print(f"Uploaded {name}")
    release = api(f"{base}/releases/{release['id']}", "PATCH", {"draft": False, "make_latest": "true"})
    print(release["html_url"])

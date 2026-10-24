//! Stamps the binary with what it was built from, as `UNDERDRAW_VERSION`: the
//! release alone for a build of the release's tag, and otherwise the release,
//! the commits past the newest tag and the commit — `0.1.0+11.g9f58035`, with
//! `.dirty` for a tree that had changes. A build with no git around it (a
//! tarball) says the release alone.

use std::process::Command;

/// Where this package is in the checkout it is built from.
const PACKAGE: &str = "crates/cli/";

fn main() {
    embed_gallery();
    let release = env!("CARGO_PKG_VERSION");
    let described = checkout().then(describe).flatten();
    println!("cargo:rustc-env=UNDERDRAW_VERSION={}", built_from(release, described.as_deref()));
    println!("cargo:rerun-if-changed=build.rs");
    for path in watched().unwrap_or_default() {
        if std::path::Path::new(&path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}

/// What `--version` says. `described` is what
/// `git describe --tags --long --dirty --always` said: `None` with no git, and
/// the commit alone where no tag is reachable.
fn built_from(release: &str, described: Option<&str>) -> String {
    let Some(described) = described else {
        return release.to_string();
    };
    let (described, dirty) = match described.strip_suffix("-dirty") {
        Some(clean) => (clean, ".dirty"),
        None => (described, ""),
    };
    // `<tag>-<distance>-g<commit>`, and a tag may itself hold a `-`.
    let mut parts = described.rsplitn(3, '-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(commit), Some(distance), Some(tag)) => {
            if distance == "0" && dirty.is_empty() && tag.strip_prefix('v') == Some(release) {
                release.to_string()
            } else {
                format!("{release}+{distance}.{commit}{dirty}")
            }
        }
        _ => format!("{release}+g{described}{dirty}"),
    }
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether the git around this build is underdraw's own checkout, and not a
/// repo a tarball of it was unpacked inside.
fn checkout() -> bool {
    git(&["rev-parse", "--show-prefix"]).is_some_and(|prefix| prefix == PACKAGE)
}

fn describe() -> Option<String> {
    git(&["describe", "--tags", "--long", "--dirty", "--always", "--match", "v*"])
}

/// What changes when the answer may: the commit checked out, the index, the
/// tags, and every tracked entry of the checkout's root.
fn watched() -> Option<Vec<String>> {
    if !checkout() {
        return None;
    }
    let dir = git(&["rev-parse", "--path-format=absolute", "--git-dir"])?;
    let common = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let mut paths = vec![
        format!("{dir}/HEAD"),
        format!("{dir}/logs/HEAD"),
        format!("{dir}/index"),
        format!("{common}/refs/tags"),
        format!("{common}/packed-refs"),
    ];
    let root = git(&["rev-parse", "--show-toplevel"])?;
    let tracked = git(&["ls-tree", "--name-only", "--full-tree", "HEAD"])?;
    paths.extend(tracked.lines().map(|entry| format!("{root}/{entry}")));
    Some(paths)
}

/// Where the gallery's own project is in the checkout, from this package:
/// the description of underdraw's own UI.
const GALLERY: &str = "../../drafts/gallery";

/// `$OUT_DIR/gallery.rs`: the gallery's own project built into the binary,
/// as the text of its domain, its tokens and every component it shares, so
/// the window draws what that project says (ADR 0024), and a component
/// added there needs no edit here.
fn embed_gallery() {
    use std::path::Path;
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let root = Path::new(&manifest).join(GALLERY);
    let components = root.join("components");
    println!("cargo:rerun-if-changed={}", components.display());
    let mut names: Vec<String> = std::fs::read_dir(&components)
        .expect("drafts/gallery/components is in the checkout")
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.strip_suffix(".json").map(str::to_string))
        .collect();
    names.sort();
    let text = |path: &Path| {
        println!("cargo:rerun-if-changed={}", path.display());
        format!("include_str!({:?})", path.display().to_string())
    };
    let mut out = format!("pub const DOMAIN: &str = {};\n", text(&root.join("domain.json")));
    out.push_str(&format!("pub const TOKENS: &str = {};\n", text(&root.join("tokens.json"))));
    out.push_str("pub const COMPONENTS: &[(&str, &str)] = &[\n");
    for name in &names {
        out.push_str(&format!("    ({name:?}, {}),\n", text(&components.join(format!("{name}.json")))));
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("gallery.rs");
    std::fs::write(dest, out).expect("OUT_DIR is writable");
}

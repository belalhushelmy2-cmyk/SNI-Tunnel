//! Thin writer around the upstream AerynOS stone format implementation.
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Read},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use stone::{
    StoneDigestWriterHasher, StoneHeaderV1FileType, StonePayloadLayoutFile as LayoutFile,
    StonePayloadLayoutRecord as Layout, StonePayloadMetaDependency as Dep,
    StonePayloadMetaPrimitive as Value, StonePayloadMetaRecord as Meta, StonePayloadMetaTag as Tag,
    StoneWriter,
};
fn visit(
    root: &Path,
    path: &Path,
    layouts: &mut Vec<Layout>,
    files: &mut BTreeMap<u128, PathBuf>,
) -> std::io::Result<()> {
    let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let meta = path.symlink_metadata()?;
        let name = path.strip_prefix(root).unwrap().to_str().unwrap();
        assert!(!name.starts_with('/') && !name.split('/').any(|p| p == ".."));
        let file = if meta.is_dir() {
            LayoutFile::Directory(name.into())
        } else if meta.is_file() {
            let mut data = Vec::new();
            File::open(&path)?.read_to_end(&mut data)?;
            let mut hash = StoneDigestWriterHasher::new();
            hash.update(&data);
            let digest = hash.digest128();
            files.entry(digest).or_insert(path.clone());
            LayoutFile::Regular(digest, name.into())
        } else if meta.is_symlink() {
            LayoutFile::Symlink(
                std::fs::read_link(&path)?.to_str().unwrap().into(),
                name.into(),
            )
        } else {
            panic!("Unsupported file type: {}", path.display())
        };
        layouts.push(Layout {
            uid: 0,
            gid: 0,
            mode: meta.permissions().mode(),
            tag: 0,
            file,
        });
        if meta.is_dir() {
            visit(root, &path, layouts, files)?;
        }
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        4,
        "Usage: sni-stone-pack ROOT OUTPUT DEPENDENCIES.txt"
    );
    let root = Path::new(&args[1]);
    let mut metadata = vec![];
    for (tag, value) in [
        (Tag::Name, "sni-tunnel-v2"),
        (Tag::Version, "2.1.5"),
        (Tag::Architecture, "x86_64"),
        (Tag::Summary, "Native VMess desktop with custom TLS SNI"),
        (
            Tag::Description,
            "Slint desktop with bundled Xray and BadVPN, Linux TUN and DNS recovery.",
        ),
        (Tag::SourceID, "sni-tunnel-v2"),
        (
            Tag::Homepage,
            "https://github.com/belalhushelmy2-cmyk/SNI-Tunnel",
        ),
        (Tag::License, "LicenseRef-SNI-Tunnel"),
    ] {
        metadata.push(Meta {
            tag,
            primitive: Value::String(value.into()),
        });
    }
    for tag in [Tag::Release, Tag::BuildRelease] {
        metadata.push(Meta {
            tag,
            primitive: Value::Uint64(1),
        });
    }
    for line in std::fs::read_to_string(&args[3])?
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (kind, name) = line.split_once(':').expect("kind:name dependency");
        let kind = match kind {
            "name" => Dep::PackageName,
            "soname" => Dep::SharedLibrary,
            "binary" => Dep::Binary,
            "sysbinary" => Dep::SystemBinary,
            _ => panic!("Unknown dependency kind"),
        };
        metadata.push(Meta {
            tag: Tag::Depends,
            primitive: Value::Dependency(kind, name.into()),
        });
    }
    metadata.push(Meta {
        tag: Tag::Provides,
        primitive: Value::Provider(Dep::Binary, "sni-tunnel-v2".into()),
    });
    let (mut layouts, mut files) = (vec![], BTreeMap::new());
    let usr = root.join("usr");
    visit(&usr, &usr, &mut layouts, &mut files)?;
    let mut writer = StoneWriter::new(File::create(&args[2])?, StoneHeaderV1FileType::Binary)?;
    writer.add_payload(metadata.as_slice())?;
    writer.add_payload(layouts.as_slice())?;
    let size = files.values().map(|p| p.metadata().unwrap().len()).sum();
    let mut writer = writer.with_content(Cursor::new(Vec::<u8>::new()), Some(size), 2)?;
    for path in files.values() {
        writer.add_content(&mut File::open(path)?)?;
    }
    writer.finalize()?;
    Ok(())
}

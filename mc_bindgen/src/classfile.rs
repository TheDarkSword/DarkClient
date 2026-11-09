//! Reading class files out of a Minecraft client jar.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use noak::mutf8::MStr;
use noak::reader::cpool::{self, ConstantPool, Index};
use noak::reader::Class as ClassReader;
use zip::ZipArchive;

use crate::dump::{Class, Dump, Kind, Member};
use crate::Error;

/// `ACC_SYNTHETIC`: compiler-generated members (lambdas, bridges, accessors)
/// are not part of any API the client could bind.
const SYNTHETIC: u16 = 0x1000;

/// A parsed class: its JNI name and its dump entry.
pub struct Parsed {
    pub name: String,
    pub class: Class,
}

fn text(value: &MStr) -> Result<String, String> {
    value
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| format!("non-UTF-8 name {value:?}"))
}

fn class_name<'a>(
    pool: &ConstantPool<'a>,
    index: Index<cpool::Class<'a>>,
) -> Result<String, String> {
    text(pool.retrieve(index).map_err(|e| e.to_string())?.name)
}

/// Parses one class file.
pub fn parse_class(bytes: &[u8]) -> Result<Parsed, String> {
    let reader = ClassReader::new(bytes).map_err(|e| e.to_string())?;
    let pool = reader.pool();

    let name = class_name(pool, reader.this_class())?;
    let mut class = Class {
        flags: Kind::Class.keep(reader.access_flags().bits()),
        superclass: reader
            .super_class()
            .map(|index| class_name(pool, index))
            .transpose()?,
        ..Class::default()
    };
    for interface in reader.interfaces().iter() {
        class
            .interfaces
            .push(class_name(pool, interface.map_err(|e| e.to_string())?)?);
    }

    for field in reader.fields().iter() {
        let field = field.map_err(|e| e.to_string())?;
        if field.access_flags().bits() & SYNTHETIC == 0 {
            class.fields.insert(Member {
                name: text(pool.retrieve(field.name()).map_err(|e| e.to_string())?)?,
                descriptor: text(
                    pool.retrieve(field.descriptor())
                        .map_err(|e| e.to_string())?,
                )?,
                flags: Kind::Field.keep(field.access_flags().bits()),
            });
        }
    }
    for method in reader.methods().iter() {
        let method = method.map_err(|e| e.to_string())?;
        if method.access_flags().bits() & SYNTHETIC == 0 {
            class.methods.insert(Member {
                name: text(pool.retrieve(method.name()).map_err(|e| e.to_string())?)?,
                descriptor: text(
                    pool.retrieve(method.descriptor())
                        .map_err(|e| e.to_string())?,
                )?,
                flags: Kind::Method.keep(method.access_flags().bits()),
            });
        }
    }
    Ok(Parsed { name, class })
}

/// Reads `name` (a JNI class name) out of `archive`, if the jar has it.
fn read_entry(archive: &mut ZipArchive<File>, name: &str) -> Result<Option<Vec<u8>>, Error> {
    let path = format!("{name}.class");
    let mut entry = match archive.by_name(&path) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(Error::Zip(path, e)),
    };
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| Error::io(&path, e))?;
    Ok(Some(bytes))
}

/// Dumps `roots` and every supertype of theirs found in `jar`. Roots the jar
/// does not contain are recorded as missing; supertypes outside it (the JDK,
/// libraries) are skipped.
pub fn dump_jar(
    jar: &Path,
    version: &str,
    jar_sha1: &str,
    roots: &[String],
) -> Result<Dump, Error> {
    let file = File::open(jar).map_err(|e| Error::io(jar, e))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| Error::Zip(jar.display().to_string(), e))?;

    let mut dump = Dump {
        version: version.to_string(),
        jar_sha1: jar_sha1.to_string(),
        ..Dump::default()
    };
    let root_set: BTreeSet<&str> = roots.iter().map(String::as_str).collect();
    let mut queue: Vec<String> = roots.to_vec();
    while let Some(name) = queue.pop() {
        if dump.classes.contains_key(&name) || dump.missing.contains(&name) {
            continue;
        }
        let Some(bytes) = read_entry(&mut archive, &name)? else {
            if root_set.contains(name.as_str()) {
                dump.missing.insert(name);
            }
            continue;
        };
        let parsed = parse_class(&bytes).map_err(|message| Error::ClassFile {
            class: name.clone(),
            message,
        })?;
        queue.extend(parsed.class.superclass.iter().cloned());
        queue.extend(parsed.class.interfaces.iter().cloned());
        dump.classes.insert(parsed.name, parsed.class);
    }
    Ok(dump)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Compiles `source` (class `Fixture`) with `javac`; `None` without a JDK.
    fn compile(source: &str) -> Option<Vec<u8>> {
        let dir = std::env::temp_dir().join(format!("mc_bindgen_fixture_{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(dir.join("Fixture.java"), source).ok()?;
        let status = Command::new("javac")
            .arg("-d")
            .arg(&dir)
            .arg(dir.join("Fixture.java"))
            .status()
            .ok()?;
        let bytes = status
            .success()
            .then(|| std::fs::read(dir.join("Fixture.class")).ok())
            .flatten();
        let _ = std::fs::remove_dir_all(&dir);
        bytes
    }

    #[test]
    fn a_compiled_class_yields_its_members_without_synthetic_ones() {
        let Some(bytes) = compile(
            "public class Fixture extends Thread implements Runnable {
                private static final int LIMIT = 3;
                public double x;
                public Fixture() {}
                public final java.lang.String name(int count) { return \"\"; }
                public Runnable task() { return () -> {}; }
            }",
        ) else {
            eprintln!("javac unavailable; skipping");
            return;
        };
        let parsed = parse_class(&bytes).unwrap();
        assert_eq!(parsed.name, "Fixture");
        assert_eq!(parsed.class.superclass.as_deref(), Some("java/lang/Thread"));
        assert_eq!(parsed.class.interfaces, ["java/lang/Runnable"]);
        assert!(parsed.class.fields.contains(&Member {
            name: "LIMIT".into(),
            descriptor: "I".into(),
            flags: 0x0002 | 0x0008 | 0x0010,
        }));
        assert!(parsed.class.methods.contains(&Member {
            name: "name".into(),
            descriptor: "(I)Ljava/lang/String;".into(),
            flags: 0x0001 | 0x0010,
        }));
        // The lambda body is synthetic and must not appear.
        assert!(parsed
            .class
            .methods
            .iter()
            .all(|method| !method.name.starts_with("lambda$")));
    }

    /// Parses every class of a real client jar — run with
    /// `MC_JAR=/path/to/client.jar cargo test -p mc_bindgen -- --ignored`
    /// to check the parser against a new Minecraft / Java release.
    #[test]
    #[ignore]
    fn every_class_of_a_real_jar_parses() {
        let jar = std::env::var("MC_JAR").expect("set MC_JAR to a client jar");
        let mut archive = ZipArchive::new(File::open(&jar).unwrap()).unwrap();
        let mut parsed = 0;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let name = entry.name().unwrap().into_owned();
            if !name.ends_with(".class") {
                continue;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if let Err(e) = parse_class(&bytes) {
                panic!("{name}: {e}");
            }
            parsed += 1;
        }
        eprintln!("parsed {parsed} classes");
        assert!(parsed > 1000);
    }
}

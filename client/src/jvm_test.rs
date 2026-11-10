//! Tests against an in-process JVM running the Java fixture in
//! `client/tests/java/` instead of the game. Needs a JDK (`javac`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use jni::objects::JValue;
use jni::{InitArgsBuilder, JNIVersion, JavaVM};

use mc_runtime::Runtime;

const SAMPLE: &str = "com/darkclient/fixture/Sample";

/// Compiles the Java fixture once and returns the output classes directory.
fn fixture_classes() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let sources_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/java");
        let out = std::env::temp_dir().join("darkclient_jvm_fixture");
        std::fs::create_dir_all(&out).expect("create the fixture output directory");

        let sources = java_sources(&sources_root);
        assert!(!sources.is_empty(), "no fixture .java sources found");

        let status = Command::new("javac")
            .arg("-d")
            .arg(&out)
            .args(&sources)
            .status()
            .expect("`javac` (a JDK) must be available to run the JVM tests");
        assert!(status.success(), "fixture compilation failed");
        out
    })
}

/// Recursively collects every `.java` file under `root`.
fn java_sources(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "java") {
                sources.push(path);
            }
        }
    }
    sources
}

/// The shared in-process JVM, created once with the fixture on its class path.
fn jvm() -> &'static JavaVM {
    static JVM: OnceLock<JavaVM> = OnceLock::new();
    JVM.get_or_init(|| {
        let classpath = format!("-Djava.class.path={}", fixture_classes().display());
        let args = InitArgsBuilder::new()
            .version(JNIVersion::V8)
            .option(&classpath)
            .build()
            .expect("build the JVM init arguments");
        JavaVM::new(args).expect("create the in-process JVM")
    })
}

fn runtime() -> &'static Runtime {
    jvm();
    Runtime::install().expect("install the runtime on the fixture JVM")
}

/// Invokes a no-argument `void` static method on the fixture `Minecraft`.
fn call_fixture_static(method: &str) {
    let mut env = jvm()
        .attach_current_thread_as_daemon()
        .expect("attach to the JVM");
    let class = env
        .find_class("net/minecraft/client/Minecraft")
        .expect("fixture Minecraft class");
    env.call_static_method(class, method, "()V", &[])
        .unwrap_or_else(|e| panic!("calling {method}: {e}"));
}

#[test]
fn resolving_a_missing_class_fails() {
    let runtime = runtime();
    let mut env = runtime.env().unwrap();
    assert!(
        runtime
            .resolve_class(&mut env, "totally/made/up/Class")
            .is_err(),
        "a class that does not exist must not resolve",
    );
}

#[test]
fn client_initializes_and_tracks_the_menu_and_world_states() {
    use crate::game::screen::MinecraftExt;

    jvm();
    crate::state::init().expect("state::init must succeed against the fixture");
    let minecraft = crate::state::minecraft();

    // A fresh init — the fixture is in its "main menu" state.
    assert!(
        minecraft.player().unwrap().is_none(),
        "no player in the menu"
    );
    assert!(minecraft.level().unwrap().is_none(), "no world in the menu");
    assert!(minecraft.game_mode().unwrap().is_none());
    assert!(!minecraft.in_world());

    // Join a world through the fixture.
    call_fixture_static("enterWorld");
    assert!(
        minecraft.player().unwrap().is_some(),
        "the player must be present once in a world",
    );
    assert!(minecraft.level().unwrap().is_some());
    assert!(minecraft.in_world());

    // Leave it again.
    call_fixture_static("leaveWorld");
    assert!(
        minecraft.player().unwrap().is_none(),
        "the player must be gone after leaving the world",
    );
    assert!(!minecraft.in_world());
}

#[test]
fn binding_targets_reach_methods_fields_and_constructors() {
    use jni::objects::JObject;
    use jni::signature::{Primitive, ReturnType};
    use mc_runtime::{Error, Field, Method};

    const VOID: ReturnType = ReturnType::Primitive(Primitive::Void);
    static CREATE: Method = Method::new(
        SAMPLE,
        "create",
        "()Lcom/darkclient/fixture/Sample;",
        true,
        ReturnType::Object,
    );
    static NEW: Method = Method::new(SAMPLE, "<init>", "()V", false, VOID);
    static SUM: Method = Method::new(
        SAMPLE,
        "sum",
        "(IJ)J",
        false,
        ReturnType::Primitive(Primitive::Long),
    );
    static COUNTER: Field = Field::new(SAMPLE, "counter", "I", false);
    static FAIL: Method = Method::new(SAMPLE, "fail", "()V", false, VOID);
    static MISSING: Method = Method::new(SAMPLE, "missing", "()V", false, VOID);

    let mut cx = runtime().cx().unwrap();
    let sample = cx
        .call(&CREATE, &JObject::null(), &[])
        .unwrap()
        .l()
        .unwrap();
    let sum = |cx: &mut mc_runtime::Cx| {
        let args = [JValue::Int(2).as_jni(), JValue::Long(40).as_jni()];
        cx.call(&SUM, &sample, &args).unwrap().j().unwrap()
    };
    assert_eq!(sum(&mut cx), 42);
    assert_eq!(sum(&mut cx), 42, "a cached id must keep working");

    assert_eq!(cx.get(&COUNTER, &sample).unwrap().i().unwrap(), 7);
    cx.set(&COUNTER, &sample, JValue::Int(9)).unwrap();
    assert_eq!(cx.get(&COUNTER, &sample).unwrap().i().unwrap(), 9);
    let fresh = cx.construct(&NEW, &[]).unwrap();
    assert_eq!(cx.get(&COUNTER, &fresh).unwrap().i().unwrap(), 7);

    assert!(matches!(
        cx.call(&FAIL, &sample, &[]),
        Err(Error::Exception { name: "fail", .. })
    ));
    assert_eq!(
        sum(&mut cx),
        42,
        "the Java exception must have been cleared"
    );
    assert!(matches!(
        cx.call(&MISSING, &sample, &[]),
        Err(Error::MemberNotFound {
            name: "missing",
            ..
        })
    ));
}

#[test]
fn self_check_finds_members_without_initializing_classes() {
    use mc_runtime::{BindingCheck, CheckKind};

    const LOUD: &str = "com/darkclient/fixture/Loud";
    let check = |kind, name, descriptor, is_static| {
        BindingCheck::member(kind, LOUD, name, descriptor, is_static, false)
    };
    let present = [
        BindingCheck::class(LOUD, false),
        check(CheckKind::Method, "sum", "(IJ)J", false),
        check(CheckKind::Method, "run", "()V", false),
        check(
            CheckKind::Method,
            "create",
            "()Lcom/darkclient/fixture/Sample;",
            true,
        ),
        check(CheckKind::Constructor, "<init>", "(I)V", false),
        check(CheckKind::Field, "counter", "I", false),
        check(CheckKind::Field, "ratio", "D", false),
        check(CheckKind::Field, "loudInitialized", "Z", true),
    ];
    let absent = [
        check(CheckKind::Method, "sum", "(IJ)J", true),
        check(CheckKind::Method, "run", "(Lno/Such;)V", false),
        check(CheckKind::Constructor, "<init>", "()V", false),
        check(CheckKind::Field, "ratio", "I", false),
        BindingCheck::class("com/darkclient/fixture/Nope", false),
        BindingCheck::member(CheckKind::Method, "org/lwjgl/Nope", "x", "()V", false, true),
    ];

    let runtime = runtime();
    let report = runtime.verify(&present).unwrap();
    assert_eq!(report.checked, present.len());
    assert!(report.missing.is_empty(), "{:?}", report.missing);

    let report = runtime.verify(&absent).unwrap();
    assert_eq!(report.missing.len(), 5, "{:?}", report.missing);
    assert_eq!(report.absent, ["org/lwjgl/Nope"]);
    assert!(report.missing[1]
        .ends_with("(java.lang.TypeNotPresentException: Type no.Such not present)"));

    let mut env = runtime.env().unwrap();
    let initialized = env
        .get_static_field(SAMPLE, "loudInitialized", "Z")
        .unwrap()
        .z()
        .unwrap();
    assert!(!initialized, "the self-check ran a static initializer");

    // FindClass does initialize: the flag above can tell.
    env.find_class(LOUD).unwrap();
    let initialized = env.get_static_field(SAMPLE, "loudInitialized", "Z");
    assert!(initialized.unwrap().z().unwrap());
}

#[test]
fn self_check_reports_what_the_fixture_lacks() {
    runtime();
    let report = mc_bindings::verify().unwrap();
    assert!(report.checked > 100);
    let missing = |what: &str| report.missing.iter().any(|m| m.starts_with(what));
    assert!(missing("net/minecraft/client/Minecraft.getDeltaTracker()"));
    assert!(!missing("net/minecraft/client/Minecraft.player:"));
    assert!(!missing("net/minecraft/client/Minecraft.getInstance()"));
    assert!(!report
        .missing
        .iter()
        .any(|m| m == "net/minecraft/client/Minecraft"));
}

fn new_object(class: &str) -> mc_bindings::Object {
    let mut env = jvm().attach_current_thread_as_daemon().unwrap();
    let local = env.new_object(class, "()V", &[]).unwrap();
    mc_bindings::Object::from_local(&local).unwrap().unwrap()
}

#[test]
fn generated_handles_follow_the_class_hierarchy() {
    use mc_bindings::{Entity, LocalPlayer, Minecraft, Player, Window};

    runtime();
    let minecraft = Minecraft::get_instance().unwrap();
    assert!(minecraft.is::<Minecraft>().unwrap());
    let window = minecraft.get_window().unwrap();
    assert!(window.is::<Window>().unwrap());

    let object = new_object("net/minecraft/client/player/LocalPlayer");
    assert!(object.cast::<Entity>().unwrap().is_some());
    let player: LocalPlayer = object.cast().unwrap().unwrap();
    let abilities = player.get_abilities().unwrap();
    abilities.set_mayfly(true).unwrap();
    assert!(abilities.mayfly().unwrap());
    assert!(!abilities.flying().unwrap());
    assert!(player.get_id().unwrap() > 0);
    let as_player: &Player = &player;
    assert!(as_player.is_same(&player));
    assert!(window.cast::<Player>().unwrap().is_none());

    // Absent from the fixture: an error, never a crash.
    assert!(minecraft.get_delta_tracker().is_err());
}

#[test]
fn generated_enums_values_and_collections_convert() {
    use mc_bindings::{ClientLevel, InteractionHand, LivingEntity, Vec3};

    runtime();
    let entity: LivingEntity = new_object("net/minecraft/world/entity/LivingEntity")
        .cast()
        .unwrap()
        .unwrap();
    assert!(!entity
        .get_item_in_hand(InteractionHand::MainHand)
        .unwrap()
        .is_empty()
        .unwrap());
    assert!(entity
        .get_item_in_hand(InteractionHand::OffHand)
        .unwrap()
        .is_empty()
        .unwrap());
    let constant = InteractionHand::OffHand.to_object().unwrap();
    assert_eq!(
        InteractionHand::from_object(&constant).unwrap(),
        InteractionHand::OffHand
    );

    assert_eq!(entity.position().unwrap(), Vec3::new(1.5, 64.0, -2.25));
    let object = Vec3::new(3.0, -1.0, 0.5).to_object().unwrap();
    assert_eq!(
        Vec3::from_object(&object).unwrap(),
        Vec3::new(3.0, -1.0, 0.5)
    );
    assert!(Vec3::from_object(&constant).is_err());

    let level: ClientLevel = new_object("net/minecraft/client/multiplayer/ClientLevel")
        .cast()
        .unwrap()
        .unwrap();
    let entities = level.entities_for_rendering().unwrap();
    assert_eq!(entities.len(), 2);
    assert!(entities[1].is::<LivingEntity>().unwrap());
}

#[test]
fn final_fields_and_constants_are_read_once() {
    use mc_bindings::{Items, Minecraft};

    runtime();
    let minecraft = Minecraft::get_instance().unwrap();
    let gui = minecraft.gui().unwrap();
    assert!(gui.is_same(&minecraft.gui().unwrap()));
    assert!(gui.screen().unwrap().is_none());
    let totem = Items::totem_of_undying().unwrap();
    assert!(totem.is_same(&Items::totem_of_undying().unwrap()));
}

// cargo test -p client --release -- --ignored binding_call_cost --nocapture
#[test]
#[ignore]
fn binding_call_cost() {
    use mc_bindings::{ClientLevel, Entity, Minecraft};
    use std::time::Instant;

    runtime();
    let entity: Entity = new_object("net/minecraft/world/entity/Entity")
        .cast()
        .unwrap()
        .unwrap();
    let minecraft = Minecraft::get_instance().unwrap();
    let level: ClientLevel = new_object("net/minecraft/client/multiplayer/ClientLevel")
        .cast()
        .unwrap()
        .unwrap();
    const CALLS: u32 = 200_000;
    let time = |what: &str, f: &mut dyn FnMut()| {
        let start = Instant::now();
        for _ in 0..CALLS {
            f();
        }
        println!("{what}: {:?}/call", start.elapsed() / CALLS);
    };
    time("warm-up", &mut || {
        entity.get_id().unwrap();
    });
    time("int method (getId)", &mut || {
        entity.get_id().unwrap();
    });
    time("value result (position → Vec3)", &mut || {
        entity.position().unwrap();
    });
    time("handle result (getWindow)", &mut || {
        minecraft.get_window().unwrap();
    });
    time("cached final field (gui)", &mut || {
        minecraft.gui().unwrap();
    });
    time("collection of 2 (entitiesForRendering)", &mut || {
        level.entities_for_rendering().unwrap();
    });
}

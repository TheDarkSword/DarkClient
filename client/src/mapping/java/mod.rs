use jni::objects::GlobalRef;

pub struct JavaList {
    pub jni_list: GlobalRef,
}

pub struct JavaSet {
    pub jni_set: GlobalRef,
}

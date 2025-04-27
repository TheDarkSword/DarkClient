use jni::objects::GlobalRef;

pub mod java;

pub struct JavaList {
    pub jni_list: GlobalRef
}

pub struct JavaSet {
    pub jni_set: GlobalRef
}
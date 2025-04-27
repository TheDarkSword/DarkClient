use jni::objects::{JObject, JObjectArray, JValueGen};
use crate::client::DarkClient;
use crate::mapping::java::{JavaList, JavaSet};

impl JavaList {
    pub fn new(jni_list: jni::objects::GlobalRef) -> JavaList {
        JavaList {
            jni_list
        }
    }

    pub fn get(&self, index: i32) -> JObject {
        let client = DarkClient::instance();
        let mut env = client.get_env().unwrap();

        env.call_method(
            self.jni_list.as_obj(),
        "get",
        "(I)Ljava/lang/Object;",
        &[JValueGen::Int(index)],
        ).unwrap().l().unwrap()
    }

    pub fn size(&self) -> i32 {
        let client = DarkClient::instance();
        let mut env = client.get_env().unwrap();

        env.call_method(
            self.jni_list.as_obj(),
        "size",
        "()I",
        &[],
        ).unwrap().i().unwrap()
    }

    pub fn to_array(&self) -> JObjectArray {
        let client = DarkClient::instance();
        let mut env = client.get_env().unwrap();

        env.call_method(
            self.jni_list.as_obj(),
        "toArray",
        "()[Ljava/lang/Object;",
        &[],
        ).unwrap().l().unwrap().into()
    }
}

impl JavaSet {

    pub fn new(jni_set: jni::objects::GlobalRef) -> JavaSet {
        JavaSet {
            jni_set
        }
    }

    pub fn contains(&self, obj: JObject) -> bool {
        let client = DarkClient::instance();
        let mut env = client.get_env().unwrap();

        env.call_method(
            self.jni_set.as_obj(),
        "contains",
        "(Ljava/lang/Object;)Z",
        &[JValueGen::Object(&obj)],
        ).unwrap().z().unwrap()
    }

    pub fn size(&self) -> i32 {
        let client = DarkClient::instance();
        let mut env = client.get_env().unwrap();

        env.call_method(
            self.jni_set.as_obj(),
        "size",
        "()I",
        &[],
        ).unwrap().i().unwrap()
    }

    pub fn to_array(&self) -> JObjectArray {
        let client = DarkClient::instance();
        let mut env = client.get_env().unwrap();

        env.call_method(
            self.jni_set.as_obj(),
        "toArray",
        "()[Ljava/lang/Object;",
        &[],
        ).unwrap().l().unwrap().into()
    }
}
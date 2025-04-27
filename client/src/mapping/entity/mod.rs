use jni::objects::GlobalRef;

pub mod entity;
pub mod player;


#[derive(Debug, Clone)]
pub struct EntityLivingBase {
    pub jni_entity_living_base: GlobalRef
}
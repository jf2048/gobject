use glib::Value;

pub trait SerializeParent {
    type SerializeParentType;
}

pub trait DeserializeParent {
    type DeserializeParentType: ParentReader;
}

pub trait ParentReader {
    fn push_values(&self, values: &mut Vec<(&'static str, Value)>);
}

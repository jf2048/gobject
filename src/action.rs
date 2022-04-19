pub trait ActionStateReturn: private::Sealed {
    type ReturnType;
}

impl<T> ActionStateReturn for Option<T> {
    type ReturnType = T;
}

mod private {
    pub trait Sealed {}
    impl<T> Sealed for Option<T> {}
}

use super::*;

impl IntoElementContent for Element {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(self);
    }
}

impl IntoElementContent for Fragment {
    fn append_to(self, children: &mut Vec<Element>) {
        children.extend(self.children);
    }
}

impl<T> IntoElementContent for Option<T>
where
    T: IntoElementContent,
{
    fn append_to(self, children: &mut Vec<Element>) {
        if let Some(content) = self {
            content.append_to(children);
        }
    }
}

impl<T> IntoElementContent for Vec<T>
where
    T: IntoElementContent,
{
    fn append_to(self, children: &mut Vec<Element>) {
        for content in self {
            content.append_to(children);
        }
    }
}

impl<T, const N: usize> IntoElementContent for [T; N]
where
    T: IntoElementContent,
{
    fn append_to(self, children: &mut Vec<Element>) {
        for content in self {
            content.append_to(children);
        }
    }
}

impl IntoElementContent for String {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(content_text(self));
    }
}

impl IntoElementContent for &'static str {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(content_text(self));
    }
}

macro_rules! impl_numeric_content {
    ($($value:ty),+ $(,)?) => {
        $(
            impl IntoElementContent for $value {
                fn append_to(self, children: &mut Vec<Element>) {
                    children.push(content_text(self.to_string()));
                }
            }
        )+
    };
}

impl_numeric_content!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);

macro_rules! impl_tuple_content {
    ($($type:ident),+ $(,)?) => {
        impl<$($type),+> IntoElementContent for ($($type,)+)
        where
            $($type: IntoElementContent),+
        {
            #[allow(non_snake_case)]
            fn append_to(self, children: &mut Vec<Element>) {
                let ($($type,)+) = self;
                $($type.append_to(children);)+
            }
        }
    };
}

impl_tuple_content!(A);
impl_tuple_content!(A, B);
impl_tuple_content!(A, B, C);
impl_tuple_content!(A, B, C, D);
impl_tuple_content!(A, B, C, D, E);
impl_tuple_content!(A, B, C, D, E, F);
impl_tuple_content!(A, B, C, D, E, F, G);
impl_tuple_content!(A, B, C, D, E, F, G, H);

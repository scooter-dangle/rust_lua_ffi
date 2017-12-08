#![allow(unused_imports)]

extern crate libc;
#[macro_use]
extern crate derive_lua_marshalling;

// procedural crates do not allow exporting code themselves, so re-export the crate and
// implement the library here.
pub use derive_lua_marshalling::*;

#[derive(Debug, Clone)]
pub struct TypeDescription {
    pub typeid: ::std::any::TypeId,
    pub dependencies: ::std::collections::HashSet<::std::any::TypeId>,
    pub typedeclaration: fn() -> String,
    pub metatype: fn() -> String,
}

pub trait Type {
    fn typename() -> String;
    fn c_typename() -> String {
        Self::typename()
    }
    fn typedeclaration() -> String {
        "".to_owned()
    }
    fn metatype() -> String {
        "".to_owned()
    }
    fn dependencies() -> Dependencies {
        vec![]
            .into_iter()
            .collect()
    }
}

pub trait FromRawConversion : Type {
    fn function() -> String;
    fn c_mut_function_argument() -> String;
    fn gc() -> bool;
}

pub trait IntoRawConversion : Type {
    fn function() -> String;
    fn c_function_argument() -> String;
    fn to_pointer() -> String;
    fn to_array() -> String;
}

pub type Dependencies = ::std::collections::HashMap<::std::any::TypeId, TypeDescription>;

pub fn make_dependencies<T: Type + 'static>() -> Dependencies {
    let typeid = ::std::any::TypeId::of::<T>();
    let mut dependencies = T::dependencies();
    let type_dependencies = dependencies.keys().cloned().collect();
    dependencies
        .insert(
            typeid,
            TypeDescription{
                typeid: ::std::any::TypeId::of::<T>(),
                dependencies: type_dependencies,
                typedeclaration: T::typedeclaration,
                metatype: T::metatype,
            });
    dependencies
}

pub fn dependency_sorted_type_descriptions<'a>(
    dependencies: &'a Dependencies) -> Vec<&'a TypeDescription> {
    let mut remaining : ::std::collections::HashSet<_> = dependencies.keys().cloned().collect();
    let mut sorted_dependencies = Vec::new();
    while !remaining.is_empty() {
        let typ = {
            let (typ, dependencies) = remaining
                .iter()
                .map(|typ| (typ, dependencies.get(typ).unwrap()) )
                .find(|&(_, dependencies)| {
                    dependencies.dependencies.is_disjoint(&remaining)
                })
                .unwrap();
            sorted_dependencies.push(dependencies);
            typ.clone()
        };
        assert!(remaining.remove(&typ));
    }
    sorted_dependencies
}

impl<T: Type + 'static> Type for Option<T> {
    fn typename() -> String {
        format!("Option_{}", T::typename())
    }
    fn typedeclaration() -> String {
        format!(
            r#"typedef struct {{
    const {} *ptr;
}} {};"#,
            <T as Type>::c_typename(),
            Self::typename())
    }
    fn metatype() -> String {
        format!(
            r#"local {} = ffi.metatype("{}", {{}})"#,
            Self::typename(),
            Self::typename())
    }
    fn dependencies() -> Dependencies {
        make_dependencies::<T>()
    }
}

impl<T: FromRawConversion + 'static> FromRawConversion for Option<T> {
    fn function() -> String {
        format!(
            r#"function(value)
    return value.ptr ~= nil and invoke(value.ptr[0], {}) or nil
end"#,
            T::function())
    }
    fn c_mut_function_argument() -> String {
        format!("{}*", <Self as Type>::c_typename())
    }
    fn gc() -> bool {
        true
    }
}

pub fn ptr_type_to_pointer<T: IntoRawConversion>() -> String {
    T::function()
}

pub fn ptr_type_to_array<T: IntoRawConversion>() -> String {
    format!(
        r#"function(value)
    local result = {{}}
    for i, value in pairs(value) do
        local tmp = invoke(value, {})
        result[i] = tmp[0]
    end
    return ffi.new("const {}[?]", #result, result)
end"#,
        T::function(),
        <T as Type>::typename())
}

pub fn immediate_type_to_array<T: IntoRawConversion>() -> String {
    format!(
        r#"function(value)
    local result = {{}}
    for i, value in pairs(value) do
        local tmp = invoke(value, {})
        result[i] = tmp
    end
    return ffi.new("const {}[?]", #result, result)
end"#,
        T::function(),
        <T as Type>::typename())
}

fn primitive_type_to_pointer<T: IntoRawConversion>() -> String {
    format!(r#"function(value)
    return ffi.new("const {}[1]", {{ value }})
end"#,
        <T as Type>::c_typename())
}

fn primitive_type_to_array<T: IntoRawConversion>() -> String {
    format!(
        r#"function(value)
    return ffi.new("const {}[?]", #value, value)
end"#,
        T::c_typename())
}

impl<T: IntoRawConversion + 'static> IntoRawConversion for Option<T> {
    fn function() -> String {
        format!(r#"
function(value)
    return {}(value ~= nil and invoke(value, {}) or nil)
end
"#,
        < Self as Type >::typename(),
        <T as IntoRawConversion>::to_pointer())
    }
    fn c_function_argument() -> String {
        format!("const {}*", <Self as Type>::c_typename())
    }
    fn to_pointer() -> String {
        ptr_type_to_pointer::<Self>()
    }
    fn to_array() -> String {
        panic!("Array of Option<T> are unreliable and have been disabled");
    }
}

impl<T: Type + 'static> Type for Vec<T> {
    fn typename() -> String {
        format!("Vec_{}", T::typename())
    }
    fn typedeclaration() -> String {
        format!(
            r#"typedef struct {{
    const {} *ptr;
    uint64_t len;
    uint64_t capacity;
}} {};"#,
            <T as Type>::c_typename(),
            Self::typename())
    }
    fn metatype() -> String {
        format!(
            r#"local {} = ffi.metatype("{}", {{}})"#,
            Self::typename(),
            Self::typename())
    }
    fn dependencies() -> Dependencies {
        make_dependencies::<T>()
    }
}

impl<T: FromRawConversion + 'static> FromRawConversion for Vec<T> {
    fn function() -> String {
        format!(
            r#"function(value)
    local ret = {{}}
    local len = tonumber(value.len)
    for i = 1,len do
        ret[i] = invoke(value.ptr[i - 1], {})
    end
    return ret
end"#,
            T::function())
    }
    fn c_mut_function_argument() -> String {
        format!("{}*", <Self as Type>::c_typename())
    }
    fn gc() -> bool {
        true
    }
}

impl<T: IntoRawConversion + 'static> IntoRawConversion for Vec<T> {
    fn function() -> String {
        format!(r#"
function(value)
    return {}(invoke(value, {}), #value, 0)
end
"#,
                < Self as Type >::typename(),
                <T as IntoRawConversion>::to_array())
    }
    fn c_function_argument() -> String {
        format!("const {}*", <Self as Type>::c_typename())
    }
    fn to_pointer() -> String {
        ptr_type_to_pointer::<Self>()
    }
    fn to_array() -> String {
        immediate_type_to_array::<Self>()
    }
}

impl Type for String {
    fn c_typename() -> String {
        "char *".to_owned()
    }
    fn typename() -> String {
        "__string_ptr".to_owned()
    }
}

impl FromRawConversion for String {
    fn function() -> String {
        "ffi.string".to_owned()
    }
    fn c_mut_function_argument() -> String {
        format!("{}", <Self as Type>::c_typename())
    }
    fn gc() -> bool {
        true
    }
}

impl IntoRawConversion for String {
    fn function() -> String {
        "function(value) return value end".to_owned()
    }
    fn c_function_argument() -> String {
        format!("const {}", <Self as Type>::c_typename())
    }
    fn to_pointer() -> String {
        primitive_type_to_pointer::<Self>()
    }
    fn to_array() -> String {
        primitive_type_to_array::<Self>()
    }
}

macro_rules! primitive_lua_from_native {
    ($([$typ:ty as $c_name:expr] )*) => {
        $(
            impl Type for $typ {
                fn typename() -> String {
                    stringify!($typ).to_owned()
                }

                fn c_typename() -> String {
                    $c_name.to_owned()
                }
            }

            impl FromRawConversion for $typ {
                fn function() -> String {
                    "function(value) return value end".to_owned()
                }
                fn gc() -> bool {
                    false
                }
                fn c_mut_function_argument() -> String {
                    format!("{}", <Self as Type>::c_typename())
                }
            }

            impl IntoRawConversion for $typ {

                fn function() -> String {
                    "function(value) return value end".to_owned()
                }
                fn c_function_argument() -> String {
                    <Self as Type>::c_typename()
                }
                fn to_pointer() -> String {
                    primitive_type_to_pointer::<Self>()
                }
                fn to_array() -> String {
                    primitive_type_to_array::<Self>()
                }
            }
        )*

    };
}

macro_rules! primitive_slice_lua_native {
    ($([$typ:ty as $c_name:expr] )*) => {
        $(
            impl<'a> Type for &'a [$typ] {
                fn typename() -> String {
                    format!("Slice_{}", stringify!($typ))
                }

                fn typedeclaration() -> String {
                    format!(r#"typedef struct {{
    const {} *ptr;
    uint64_t len;
}} {};"#,
                        $c_name, Self::typename())
                }
                fn metatype() -> String {
                    format!(
                        r#"local {} = ffi.metatype("{}", {{}})"#,
                        Self::typename(),
                        Self::typename())
                }
            }
        )*
    };
}

macro_rules! primitive_slice_lua_to_native {
    ($([$typ:ty as $c_name:expr] )*) => {
        $(
            impl<'a> IntoRawConversion for &'a [$typ] {

                fn function() -> String {
                    format!(
            r#"function(value)
    local result = {{}}
    for i, value in pairs(value) do
        result[i] = value
    end
    return {}(ffi.new("{}[?]", #result, result), #result)
end"#,
                        <Self as Type>::typename(),
                        <$typ as IntoRawConversion>::c_function_argument())
                }
                fn c_function_argument() -> String {
                    format!("const {}*", <Self as Type>::c_typename())
                }
                fn to_pointer() -> String {
                    ptr_type_to_pointer::<Self>()
                }
                fn to_array() -> String {
                    ptr_type_to_array::<Self>()
                }
            }
        )*
    };
}

primitive_lua_from_native!(
    [i8 as "int8_t"]
    [i16 as "int16_t"]
    [i32 as "int32_t"]
    [i64 as "int64_t"]
    [u8 as "uint8_t"]
    [u16 as "uint16_t"]
    [u32 as "uint32_t"]
    [u64 as "uint64_t"]
    [f32 as "float"]
    [f64 as "double"]
);

primitive_slice_lua_native!(
    [i8 as "int8_t"]
    [i16 as "int16_t"]
    [i32 as "int32_t"]
    [i64 as "int64_t"]
    [u8 as "uint8_t"]
    [u16 as "uint16_t"]
    [u32 as "uint32_t"]
    [u64 as "uint64_t"]
    [f32 as "float"]
    [f64 as "double"]
);

primitive_slice_lua_to_native!(
    [i8 as "int8_t"]
    [i16 as "int16_t"]
    [i32 as "int32_t"]
    [i64 as "int64_t"]
    [u16 as "uint16_t"]
    [u32 as "uint32_t"]
    [u64 as "uint64_t"]
    [f32 as "float"]
    [f64 as "double"]
);

impl<'a> IntoRawConversion for &'a [u8] {
    fn function() -> String {
        format!(
            r#"function(value)
    if type(value) == "string" then
        return {}(value, #value)
    else
        local result = {{}}
        for i, value in pairs(value) do
            result[i] = value
        end
        return {}(ffi.new("{}[?]", #result, result), #result)
    end
end"#,
            <Self as Type>::typename(),
            <Self as Type>::typename(),
            <u8 as IntoRawConversion>::c_function_argument())
    }
    fn c_function_argument() -> String {
        format!("const {}*", <Self as Type>::c_typename())
    }
    fn to_pointer() -> String {
        ptr_type_to_pointer::<Self>()
    }
    fn to_array() -> String {
        ptr_type_to_array::<Self>()
    }
}

impl<'a> Type for &'a str {
    fn typename() -> String {
        "_str_ptr__".to_owned()
    }
    fn c_typename() -> String {
        "char *".to_owned()
    }
}

impl<'a> IntoRawConversion for &'a str {
    fn function() -> String {
        "function(value) return value end".to_owned()
    }
    fn c_function_argument() -> String {
        format!("const {}", <Self as Type>::c_typename())
    }
    fn to_pointer() -> String {
        primitive_type_to_pointer::<Self>()
    }
    fn to_array() -> String {
        primitive_type_to_array::<Self>()
    }
}

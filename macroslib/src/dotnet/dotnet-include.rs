// It is currently unused.
mod swig_foreign_types_map {}

foreign_typemap!(
    (r_type) f32;
    (f_type) "float";
);

foreign_typemap!(
    (r_type) f64;
    (f_type) "double";
);

foreign_typemap!(
    (r_type) ();
    (f_type) "void";
);

foreign_typemap!(
    (r_type) i8;
    (f_type) "sbyte";
);

foreign_typemap!(
    (r_type) u8;
    (f_type) "byte";
);

foreign_typemap!(
    (r_type) i16;
    (f_type) "short";
);

foreign_typemap!(
    (r_type) u16;
    (f_type) "ushort";
);

foreign_typemap!(
    (r_type) i32;
    (f_type) "int";
);

foreign_typemap!(
    (r_type) u32;
    (f_type) "uint";
);

foreign_typemap!(
    (r_type) i64;
    (f_type) "long";
);

foreign_typemap!(
    (r_type) u64;
    (f_type) "ulong";
);

foreign_typemap!(
    ($p:r_type) usize => u64 {
        $out = $p as u64;
    };
    ($p:r_type) usize <= u64 {
        $out = $p as usize;
    };
    ($p:f_type) => "/* usize */ ulong" "$p";
    ($p:f_type) <= "/* usize */ ulong" "$p";
);

foreign_typemap!(
    ($p:r_type) isize => i64 {
        $out = $p as i64;
    };
    ($p:r_type) isize <= i64 {
        $out = $p as isize;
    };
    ($p:f_type) => "/* isize */ long" "$p";
    ($p:f_type) <= "/* isize */ long" "$p";
);

foreign_typemap!(
    (r_type) /* c_str_u16 */ *mut u16;
    (f_type) "/* mut c_str_u16 */ IntPtr";
);

foreign_typemap!(
    (r_type) /* c_str_u16 */ *const u16;
    (f_type) "/* const c_str_u16 */ IntPtr";
);

foreign_typemap!(
    ($p:r_type) bool => u8 {
        $out = if $p  { 1 } else { 0 };
    };
    ($p:f_type) => "bool" "($p != 0)";
    ($p:r_type) bool <= u8 {
        $out = $p != 0;
    };
    ($p:f_type) <= "bool" "(byte) ($p ? 1 : 0)";
);

// .NET prefers UTF16, but Rust doesn't provide CString/OSString equivalent that supports UTF16 on Linux.
// We need to go a bit lower.

unsafe fn c_str_u16_len(mut c_str_u16_ptr: *const u16) -> usize {
    let mut len = 0;
    while *c_str_u16_ptr != 0 {
        len += 1;
        c_str_u16_ptr = c_str_u16_ptr.offset(1);
    }
    len
}

fn alloc_c_str_u16(string: &str) -> *const u16 {
    let mut bytes_vec: Vec<u16> = string.encode_utf16().collect();
    // Add terminate NULL character
    bytes_vec.push(0);
    let boxed_slice = bytes_vec.into_boxed_slice();
    let slice_ptr = Box::into_raw(boxed_slice);
    unsafe {
        (*slice_ptr).as_ptr()
    }
}

#[allow(non_snake_case)]
#[no_mangle]
unsafe extern "C" fn c_str_u16_to_string(c_str_u16_ptr: *const u16) -> *mut String {
    if c_str_u16_ptr.is_null() {
        return ::std::ptr::null_mut();
    }
    let len = c_str_u16_len(c_str_u16_ptr);
    let slice = ::std::slice::from_raw_parts(c_str_u16_ptr, len);
    Box::into_raw(Box::new(String::from_utf16_lossy(slice)))
}

#[allow(non_snake_case)]
#[no_mangle]
unsafe extern "C" fn c_string_delete(c_str_u16: *mut u16) {
    let size = c_str_u16_len(c_str_u16) + 1; // Add NULL character size.
    let slice_ptr = ::std::ptr::slice_from_raw_parts_mut(c_str_u16, size);
    let boxed_slice: Box<[u16]> = Box::from_raw(slice_ptr);
    ::std::mem::drop(boxed_slice);
}

foreign_typemap!(
    (r_type) *mut String;
    (f_type) "/* RustString */ IntPtr";
);

foreign_typemap!(
    ($p:r_type) String => /* c_str_u16 */ *const u16 {
        $out = alloc_c_str_u16(&$p);
    };
    ($p:f_type) => "string" "RustString.rust_to_dotnet($p)";
    ($p:r_type) String <= *mut String {
        $out = unsafe { *Box::from_raw($p) };
    };
    ($p:f_type) <= "string" "RustString.dotnet_to_rust($p)";
);

foreign_typemap!(
    ($p:r_type) String => &str {
        $out = & $p;
    };
);

foreign_typemap!(
    ($p:r_type) <T> Vec<T> => &[T] {
        $out = & $p;
    };
);

foreign_typemap!(
    ($p:r_type) &str => String {
        $out = $p.to_owned();
    };
);

foreign_typemap!(
    ($p:r_type) <T> &[T] => Vec<T> {
        $out = $p.to_owned();
    };
);

#[allow(dead_code)]
pub trait SwigForeignEnum: Sized {
    fn from_u32(x: u32) -> Self;
    fn as_u32(&self) -> u32;
}

foreign_typemap!(
    ($p:r_type) /* Option */ *mut ::std::ffi::c_void;
    ($p:f_type) "/* Option */ IntPtr";
);

foreign_typemap!(
    generic_alias!(RustOptionT = swig_concat_idents!(RustOption, swig_f_type!(T)));
    generic_alias!(RustOptionT_new_none = swig_concat_idents!(RustOption, swig_f_type!(T), _new_none));
    generic_alias!(RustOptionT_new_some = swig_concat_idents!(RustOption, swig_f_type!(T), _new_some));
    generic_alias!(RustOptionT_is_some = swig_concat_idents!(RustOption, swig_f_type!(T), _is_some));
    generic_alias!(RustOptionT_take = swig_concat_idents!(RustOption, swig_f_type!(T), _take));

    define_c_type!(
        module = "RustOptionT!()";

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustOptionT_new_none!()() -> *mut Option<swig_i_type!(T)> {
            Box::into_raw(Box::new(None))
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustOptionT_new_some!()(value_0: swig_i_type!(T)) -> *mut Option<swig_i_type!(T)> {
            Box::into_raw(Box::new(Some(value_0)))
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustOptionT_is_some!()(opt: *mut Option<swig_i_type!(T)>) -> u8 {
            if (*opt).is_some() { 1 } else { 0 }
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustOptionT_take!()(opt: *mut Option<swig_i_type!(T)>) -> swig_i_type!(T) {
            let ret_0 = Box::from_raw(opt).expect("RustOptionT_take!(): trying to take the value from Option::None");
            ret_0
        }
    );

    foreign_code!(
        module = "Option<T>";
        r#"

        public class Option<T> {
        
            [System.Serializable]
            public class OptionNoneException : System.Exception
            {
                public OptionNoneException() :
                    base("Trying to get the value of an `Option` that is `None`") 
                {
                }
            }
        
            private T value;
            private bool isSome;
        
            public bool IsSome
            {
                get
                {
                    return isSome;
                }
            }
        
            public T Value
            {
                get {
                    if (!isSome) {
                        throw new OptionNoneException();
                    }
                    return value;
                }
            }
        
            public Option()
            {
                value = default(T);
                isSome = false;
            }
        
            public Option(T value)
            {
                if (value == null) 
                {
                    this.value = value;
                    this.isSome = false;
                }
                else
                {
                    this.value = value;
                    this.isSome = true;
                }
            }
        }        
        "#
    );

    foreign_code!(
        module = "RustOptionT!()";
        r#"
    internal static class RustOptionT!() {
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr RustOptionT_new_none!()();

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr RustOptionT_new_some!()(swig_i_type!(T) value);
        
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern swig_i_type!(T) RustOptionT_take!()(IntPtr optPtr);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte RustOptionT_is_some!()(IntPtr optPtr);

        internal static Option<swig_f_type!(T)> rust_to_dotnet(IntPtr optPtr)
        {
            if (RustOptionT_is_some!()(optPtr) != 0)
            {
                var value_0 = RustOptionT_take!()(optPtr);
                var value_1 = swig_foreign_from_i_type!(T, value_0);
                return new Option<swig_f_type!(T)>(value_1);
            }
            else
            {
                return new Option<swig_f_type!(T)>();
            }
        }

        internal static IntPtr dotnet_to_rust(Option<swig_f_type!(T)> opt)
        {
            if (opt.IsSome)
            {
                var value_0 = swig_foreign_to_i_type!(T, opt.Value);
                return RustOptionT_new_some!()(value_0);
            }
            else
            {
                return RustOptionT_new_none!()();
            }
        }
    }
    "#);

    ($p:r_type) <T> Option<T> => /* Option */ *mut ::std::ffi::c_void {
        let $p: Option<swig_i_type!(T)> = $p.map(|value_0| {
            swig_from_rust_to_i_type!(T, value_0, value_1)
            value_1
        });
        $out = Box::into_raw(Box::new($p)) as *mut ::std::ffi::c_void;
    };
    ($p:f_type) => "Option<swig_f_type!(T)>" "RustOptionT!().rust_to_dotnet($p)";
    ($p:r_type) <T> Option<T> <= /* Option */ *mut ::std::ffi::c_void {
        let $p: Box<Option<swig_i_type!(T)>> = unsafe { Box::from_raw($p as *mut Option<swig_i_type!(T)>) };
        $out = $p.map(|value_0| {
            swig_from_i_type_to_rust!(T, value_0, value_1)
            value_1
        });
    };
    ($p:f_type) <= "Option<swig_f_type!(T)>" "RustOptionT!().dotnet_to_rust($p)";

);

foreign_typemap!(
    ($p:r_type) /* Vec */ *mut ::std::ffi::c_void;
    ($p:f_type) "/* RustVec */ IntPtr";
);

foreign_typemap!(
    ($p:r_type) /* Iter */ *mut ::std::ffi::c_void;
    ($p:f_type) "/* Iter */ IntPtr";
);

foreign_typemap!(
    generic_alias!(RustVecT = swig_concat_idents!(RustVec, swig_f_type!(T)));
    generic_alias!(RustVecT_new = swig_concat_idents!(RustVec, swig_f_type!(T), _new));
    generic_alias!(RustVecT_push = swig_concat_idents!(RustVec, swig_f_type!(T), _push));
    generic_alias!(RustVecT_iter_next = swig_concat_idents!(RustVec, swig_f_type!(T), _iter_next));
    generic_alias!(RustVecT_iter_delete = swig_concat_idents!(RustVec, swig_f_type!(T), _iter_delete));
    generic_alias!(RustVecT_option_is_some = swig_concat_idents!(RustVec, swig_f_type!(T), _option_is_some));
    generic_alias!(RustVecT_option_take = swig_concat_idents!(RustVec, swig_f_type!(T), _option_take));

    ($p:r_type) <T> Vec<T> => /* Iter */ *mut ::std::ffi::c_void {
        let $p: Vec<swig_i_type!(T)> = $p.into_iter().map(|e_0| {
            swig_from_rust_to_i_type!(T, e_0, e_1)
            e_1
        }).collect();
        let $p: std::vec::IntoIter<swig_i_type!(T)> = $p.into_iter();
        $out = Box::into_raw(Box::new($p)) as *mut ::std::ffi::c_void;
    };
    ($p:f_type) => "System.Collections.Generic.List<swig_f_type!(T)>" "RustVecT!().rust_to_dotnet($p)";
    ($p:r_type) <T> Vec<T> <= /* Vec */ *mut ::std::ffi::c_void {
        let $p = unsafe { *Box::from_raw($p as *mut Vec<swig_i_type!(T)>) };
        $out = $p.into_iter().map(|e_0| {
            swig_from_i_type_to_rust!(T, e_0, e_1)
            e_1
        }).collect();
    };
    ($p:f_type) <= "System.Collections.Generic.List<swig_f_type!(T)>" "RustVecT!().dotnet_to_rust($p)";

    define_c_type!(
        module = "RustVecT!()";

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_new!()() -> *mut Vec<swig_i_type!(T)> {
            Box::into_raw(Box::new(Vec::new()))
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_push!()(vec: *mut Vec<swig_i_type!(T)>, element: swig_i_type!(T)) {
            assert!(!vec.is_null());
            (*vec).push(element);
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_iter_next!()(iter: *mut std::vec::IntoIter<swig_i_type!(T)>) -> *mut Option<swig_i_type!(T)> {
            assert!(!iter.is_null());
            let mut iter = &mut *iter;
            Box::into_raw(Box::new(iter.next()))
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_iter_delete!()(iter: *mut std::vec::IntoIter<swig_i_type!(T)>) {
            assert!(!iter.is_null());
            ::std::mem::drop(Box::from_raw(iter));
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_option_is_some!()(opt: *mut Option<swig_i_type!(T)>) -> u8 {
            if (*opt).is_some() { 1 } else { 0 }
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_option_take!()(opt: *mut Option<swig_i_type!(T)>) -> swig_i_type!(T) {
            let ret_0 = Box::from_raw(opt).expect("RustVecT_option_take!(): trying to take the value from Option::None");
            ret_0
        }
    );

    foreign_code!(
        module = "RustVecT!()";
        r#"
    public static class RustVecT!() {
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr RustVecT_new!()();
        
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void RustVecT_push!()(IntPtr vecPtr, swig_i_type!(T) element);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern /* Option<i_type> */ IntPtr RustVecT_iter_next!()(IntPtr iterPtr);
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void RustVecT_iter_delete!()(IntPtr iterPtr);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern swig_i_type!(T) RustVecT_option_take!()(IntPtr optPtr);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte RustVecT_option_is_some!()(IntPtr optPtr);


        internal static System.Collections.Generic.List<swig_f_type!(T)> rust_to_dotnet(IntPtr iterPtr) {
            var list = new System.Collections.Generic.List<swig_f_type!(T)>();
            while (true)
            {
                var next_rust_opt = RustVecT!().RustVecT_iter_next!()(iterPtr);
                if (RustVecT_option_is_some!()(next_rust_opt) == 0)
                {
                    break;
                }
                var value_rust = RustVecT_option_take!()(next_rust_opt);
                var value = swig_foreign_from_i_type!(T, value_rust);
                list.Add(value);
            }
            RustVecT_iter_delete!()(iterPtr);
            return list;
        }

        internal static IntPtr dotnet_to_rust(System.Collections.Generic.List<swig_f_type!(T)> list) {
            var vec = RustVecT_new!()();
            foreach (var element in list)
            {
                var i_element = swig_foreign_to_i_type!(T, element);
                RustVecT!().RustVecT_push!()(vec, i_element);
            }
            return vec;
        }
    }
        "#
    );
);

// Slices
foreign_typemap!(
    ($p:r_type) /* SliceVec */ *mut ::std::ffi::c_void;
    ($p:f_type) "/* RustSlice */ IntPtr";
);

foreign_typemap!(
    ($p:r_type) /* SliceIter */ *mut ::std::ffi::c_void;
    ($p:f_type) "/* SliceIter */ IntPtr";
);

foreign_typemap!(
    generic_alias!(RustVecT = swig_concat_idents!(RustVec, swig_f_type!(T)));
    generic_alias!(RustVecT_new = swig_concat_idents!(RustVec, swig_f_type!(T), _new));
    generic_alias!(RustVecT_push = swig_concat_idents!(RustVec, swig_f_type!(T), _push));
    generic_alias!(RustVecT_iter_next = swig_concat_idents!(RustVec, swig_f_type!(T), _iter_next));
    generic_alias!(RustVecT_iter_delete = swig_concat_idents!(RustVec, swig_f_type!(T), _iter_delete));
    generic_alias!(RustVecT_option_is_some = swig_concat_idents!(RustVec, swig_f_type!(T), _option_is_some));
    generic_alias!(RustVecT_option_take = swig_concat_idents!(RustVec, swig_f_type!(T), _option_take));

    ($p:r_type) <T> &[T] => /* SliceIter */ *mut ::std::ffi::c_void {
        let $p: Vec<swig_i_type!(T)> = $p.to_owned().into_iter().map(|e_0| {
            swig_from_rust_to_i_type!(T, e_0, e_1)
            e_1
        }).collect();
        let $p: std::vec::IntoIter<swig_i_type!(T)> = $p.into_iter();
        $out = Box::into_raw(Box::new($p)) as *mut ::std::ffi::c_void;
    };
    ($p:f_type) => "/* Slice */ System.Collections.Generic.List<swig_f_type!(T)>" "RustVecT!().rust_to_dotnet($p)";
    ($p:r_type) <T> &[T] <= /* SliceVec */ *mut ::std::ffi::c_void {
        let $p = unsafe { *Box::from_raw($p as *mut Vec<swig_i_type!(T)>) };
        let $p = $p.into_iter().map(|e_0| {
            swig_from_i_type_to_rust!(T, e_0, e_1)
            e_1
        }).collect::<Vec<_>>();
        $out = $p.as_ref();
    };
    ($p:f_type) <= "/* Slice */ System.Collections.Generic.List<swig_f_type!(T)>" "RustVecT!().dotnet_to_rust($p)";

    define_c_type!(
        module = "RustVecT!()";

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_new!()() -> *mut Vec<swig_i_type!(T)> {
            Box::into_raw(Box::new(Vec::new()))
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_push!()(vec: *mut Vec<swig_i_type!(T)>, element: swig_i_type!(T)) {
            assert!(!vec.is_null());
            (*vec).push(element);
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_iter_next!()(iter: *mut std::vec::IntoIter<swig_i_type!(T)>) -> *mut Option<swig_i_type!(T)> {
            assert!(!iter.is_null());
            let mut iter = &mut *iter;
            Box::into_raw(Box::new(iter.next()))
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_iter_delete!()(iter: *mut std::vec::IntoIter<swig_i_type!(T)>) {
            assert!(!iter.is_null());
            ::std::mem::drop(Box::from_raw(iter));
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_option_is_some!()(opt: *mut Option<swig_i_type!(T)>) -> u8 {
            if (*opt).is_some() { 1 } else { 0 }
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustVecT_option_take!()(opt: *mut Option<swig_i_type!(T)>) -> swig_i_type!(T) {
            let ret_0 = Box::from_raw(opt).expect("RustVecT_option_take!(): trying to take the value from Option::None");
            ret_0
        }
    );

    foreign_code!(
        module = "RustVecT!()";
        r#"
    public static class RustVecT!() {
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr RustVecT_new!()();
        
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void RustVecT_push!()(IntPtr vecPtr, swig_i_type!(T) element);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern /* Option<i_type> */ IntPtr RustVecT_iter_next!()(IntPtr iterPtr);
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void RustVecT_iter_delete!()(IntPtr iterPtr);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern swig_i_type!(T) RustVecT_option_take!()(IntPtr optPtr);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte RustVecT_option_is_some!()(IntPtr optPtr);

        internal static System.Collections.Generic.List<swig_f_type!(T)> rust_to_dotnet(IntPtr iterPtr) {
            var list = new System.Collections.Generic.List<swig_f_type!(T)>();
            while (true)
            {
                var next_rust_opt = RustVecT!().RustVecT_iter_next!()(iterPtr);
                if (RustVecT_option_is_some!()(next_rust_opt) == 0)
                {
                    break;
                }
                var value_rust = RustVecT_option_take!()(next_rust_opt);
                var value = swig_foreign_from_i_type!(T, value_rust);
                list.Add(value);
            }
            RustVecT_iter_delete!()(iterPtr);
            return list;
        }

        internal static IntPtr dotnet_to_rust(System.Collections.Generic.List<swig_f_type!(T)> list) {
            var vec = RustVecT_new!()();
            foreach (var element in list)
            {
                var i_element = swig_foreign_to_i_type!(T, element);
                RustVecT!().RustVecT_push!()(vec, i_element);
            }
            return vec;
        }
    }
        "#
    );
);


foreign_typemap!(
    ($p:r_type) /* ResultVoid */ *mut ::std::ffi::c_void;
    ($p:f_type) "/* ResultVoid */ IntPtr";
);

foreign_typemap!(

    define_c_type!(
        module = "RustResultVoid";

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustResultVoid_is_ok(opt: *mut Result<(), String>) -> u8 {
            if (*opt).is_ok() { 1 } else { 0 }
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustResultVoid_take_err(result: *mut Result<(), String>) -> /* c_str_u16 */ *const u16 {
            let ret_0 = Box::from_raw(result).expect_err("RustResultVoid_take_err: trying to take the error from Result::Ok");
            alloc_c_str_u16(&ret_0)
        }
    );

    foreign_code!(
        module = "RustResultVoid";
        r#"
    internal static class RustResultVoid {

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte RustResultVoid_is_ok(IntPtr resultPtr);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern /* mut c_str_u16 */ IntPtr RustResultVoid_take_err(IntPtr resultPtr);

        internal static void unwrap(IntPtr resultPtr)
        {
            if (RustResultVoid_is_ok(resultPtr) != 0)
            {
                return;
            }
            else
            {
                var messagePtr = RustResultVoid_take_err(resultPtr);
                var message = RustString.rust_to_dotnet(messagePtr);
                throw new Error(message);
            }
        }
    }
    "#);
    ($p:r_type) <T> Result<(), T> => /* ResultVoid */ *mut ::std::ffi::c_void {
        let $p: Result<(), String> = $p.map_err(|err| {
            swig_collect_error_message(&err)
        });
        $out = Box::into_raw(Box::new($p)) as *mut ::std::ffi::c_void;
    };
    ($p:f_type) => "/* ResultVoid<swig_subst_type!(T)> */ void" "RustResultVoid.unwrap($p)";
);


foreign_typemap!(
    ($p:r_type) /* Result */ *mut ::std::ffi::c_void;
    ($p:f_type) "/* Result */ IntPtr";
);

foreign_typemap!(
    generic_alias!(RustResultT = swig_concat_idents!(RustResult, swig_f_type!(T1)));
    generic_alias!(RustResultT_is_ok = swig_concat_idents!(RustResult, swig_f_type!(T1), _is_ok));
    generic_alias!(RustResultT_take_ok = swig_concat_idents!(RustResult, swig_f_type!(T1), _take_ok));
    generic_alias!(RustResultT_take_err = swig_concat_idents!(RustResult, swig_f_type!(T1), _take_error));

    define_c_type!(
        module = "RustResultT!()";

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustResultT_is_ok!()(opt: *mut Result<swig_i_type!(T1), String>) -> u8 {
            if (*opt).is_ok() { 1 } else { 0 }
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustResultT_take_ok!()(result: *mut Result<swig_i_type!(T1), String>) -> swig_i_type!(T1) {
            let ret_0 = Box::from_raw(result).expect("RustResultT_take_ok!(): trying to take the value from Result::Err");
            ret_0
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustResultT_take_err!()(result: *mut Result<swig_i_type!(T1), String>) -> /* c_str_u16 */ *const u16 {
            let ret_0 = Box::from_raw(result).expect_err("RustResultT_take_err!(): trying to take the error from Result::Ok");
            alloc_c_str_u16(&ret_0)
        }
    );

    foreign_code!(
        module = "RustResultT!()";
        r#"
    internal static class RustResultT!() {

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte RustResultT_is_ok!()(IntPtr resultPtr);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern swig_i_type!(T1) RustResultT_take_ok!()(IntPtr resultPtr);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern /* mut c_str_u16 */ IntPtr RustResultT_take_err!()(IntPtr resultPtr);

        internal static swig_f_type!(T1) unwrap(IntPtr resultPtr)
        {
            if (RustResultT_is_ok!()(resultPtr) != 0)
            {
                var value_0 = RustResultT_take_ok!()(resultPtr);
                var value_1 = swig_foreign_from_i_type!(T1, value_0);
                return value_1;
            }
            else
            {
                var messagePtr = RustResultT_take_err!()(resultPtr);
                var message = RustString.rust_to_dotnet(messagePtr);
                throw new Error(message);
            }
        }
    }
    "#);
    ($p:r_type) <T1, T2> Result<T1, T2> => /* Result */ *mut ::std::ffi::c_void {
        let $p: Result<swig_i_type!(T1), String> = $p.map(|ok_0| {
            swig_from_rust_to_i_type!(T1, ok_0, ok_1)
            ok_1
        }).map_err(|err| {
            swig_collect_error_message(&err)
        });
        $out = Box::into_raw(Box::new($p)) as *mut ::std::ffi::c_void;
    };
    ($p:f_type) => "/* Result<swig_subst_type!(T1), swig_subst_type!(T2)> */ swig_f_type!(T1)" "RustResultT!().unwrap($p)";
);

fn swig_collect_error_message(error: &dyn std::error::Error) -> String {
    if let Some(source) = error.source() {
        format!("{}\nCaused by:\n{}", error, swig_collect_error_message(source))
    } else {
        error.to_string()
    }
}

foreign_typemap!(
    (r_type) /* Tuple */ *mut ::std::ffi::c_void;
    (f_type) "/* Tuple */ IntPtr";
);

foreign_typemap!(
    generic_alias!(RustTuple2T = swig_concat_idents!(RustTuple2T, swig_f_type!(T1), swig_f_type!(T2)));
    generic_alias!(RustTuple2T_new = swig_concat_idents!(RustTuple2T, swig_f_type!(T1), swig_f_type!(T2), _new));
    generic_alias!(RustTuple2T_delete = swig_concat_idents!(RustTuple2T, swig_f_type!(T1), swig_f_type!(T2), _delete));
    generic_alias!(RustTuple2T_take_1 = swig_concat_idents!(RustTuple2T, swig_f_type!(T1), swig_f_type!(T2), _take_1));
    generic_alias!(RustTuple2T_take_2 = swig_concat_idents!(RustTuple2T, swig_f_type!(T1), swig_f_type!(T2), _take_2));

    define_c_type!(
        module = "RustTuple2T!()";

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustTuple2T_new!()(t_1: swig_i_type!(T1), t_2: swig_i_type!(T2)) -> *mut (swig_i_type!(T1), swig_i_type!(T2)) {
            Box::into_raw(Box::new((t_1, t_2)))
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustTuple2T_take_1!()(tuple: *mut (swig_i_type!(T1), swig_i_type!(T2))) -> swig_i_type!(T1) {
            (*tuple).0
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustTuple2T_take_2!()(tuple: *mut (swig_i_type!(T1), swig_i_type!(T2))) -> swig_i_type!(T2) {
            (*tuple).1
        }

        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn RustTuple2T_delete!()(tuple: *mut (swig_i_type!(T1), swig_i_type!(T2))) {
            // We assume that members of tuple were already "taken", so there's no need to drop them.
            ::std::mem::drop(Box::from_raw(tuple));
        }
    );

    foreign_code!(
        module = "RustTuple2T!()";
        r#"
    internal static class RustTuple2T!() {

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern /* Tuple */ IntPtr RustTuple2T_new!()(swig_i_type!(T1) t_1, swig_i_type!(T2) t_2);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern swig_i_type!(T1) RustTuple2T_take_1!()(IntPtr tuple);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern swig_i_type!(T2) RustTuple2T_take_2!()(IntPtr tuple);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void RustTuple2T_delete!()(IntPtr tuple);

        internal static Tuple<swig_f_type!(T1), swig_f_type!(T2)> rust_to_dotnet(IntPtr rustTuple)
        {
            var t_1_rust = RustTuple2T_take_1!()(rustTuple);
            var t_1 = swig_foreign_from_i_type!(T1, t_1_rust);
            var t_2_rust = RustTuple2T_take_2!()(rustTuple);
            var t_2 = swig_foreign_from_i_type!(T2, t_2_rust);
            var ret = Tuple.Create(t_1, t_2);
            RustTuple2T_delete!()(rustTuple);
            return ret;
        }
        internal static /* Tuple */ IntPtr dotnet_to_rust(Tuple<swig_f_type!(T1), swig_f_type!(T2)> tuple)
        {
            var t_1 = tuple.Item1;
            var t_1_rust = swig_foreign_to_i_type!(T1, t_1);
            var t_2 = tuple.Item2;
            var t_2_rust = swig_foreign_to_i_type!(T2, t_2);
            // We don't call delete in `Input` scenario. Rust-side conversion code will take care of it.
            return RustTuple2T_new!()(t_1_rust, t_2_rust);            
        }
    }
    "#);

    ($p:r_type) <T1, T2> (T1, T2) => /* Tuple */ *mut ::std::ffi::c_void {
        let (t_1, t_2) = $p;
        swig_from_rust_to_i_type!(T1, t_1, t_1_i)
        swig_from_rust_to_i_type!(T2, t_2, t_2_i)
        $out = Box::into_raw(Box::new((t_1_i, t_2_i))) as *mut ::std::ffi::c_void;
    };
    ($p:f_type) => "Tuple<swig_f_type!(T1), swig_f_type!(T2)>" "RustTuple2T!().rust_to_dotnet($p)";
    ($p:r_type) <T1, T2> (T1, T2) <= /* Tuple */ *mut ::std::ffi::c_void {
        assert!(!$p.is_null());
        let tuple_i_ptr = $p as *mut (swig_subst_type!(T1), swig_subst_type!(T2));
        let tuple_i = unsafe { Box::from_raw(tuple_i_ptr) };
        let (t_1_i, t_2_i) = tuple_i;
        swig_from_i_type_to_rust!(T1, t_1_i, t_1)
        swig_from_i_type_to_rust!(T2, t_2_i, t_2)
        $out = (t_1, t_2);
    };
    ($p:f_type) <= "Tuple<swig_f_type!(T1), swig_f_type!(T2)>" "RustTuple2T!().dotnet_to_rust($p)";

);

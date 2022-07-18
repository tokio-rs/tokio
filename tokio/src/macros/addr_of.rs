//! This module defines a macro that lets you go from a raw pointer to a struct
//! to a raw pointer to a field of the struct.

#[cfg(not(tokio_no_addr_of))]
macro_rules! generate_addr_of_methods {
    (
    impl<$($gen:ident)*> $struct_name:ty {$(
        $(#[$attrs:meta])*
        $vis:vis unsafe fn $fn_name:ident(self: NonNull<Self>) -> NonNull<$field_type:ty> {
            &self$(.$field_name:tt)+
        }
    )*}
    ) => {
        impl<$($gen)*> $struct_name {$(
            $(#[$attrs])*
            $vis unsafe fn $fn_name(me: ::core::ptr::NonNull<Self>) -> ::core::ptr::NonNull<$field_type> {
                let me = me.as_ptr();
                let field = ::std::ptr::addr_of_mut!((*me) $(.$field_name)+ );
                ::core::ptr::NonNull::new_unchecked(field)
            }
        )*}
    };
}

// The `addr_of_mut!` macro is only available for MSRV at least 1.51.0. This
// version of the macro uses a workaround for older versions of rustc.
#[cfg(tokio_no_addr_of)]
macro_rules! generate_addr_of_methods {
    (
    impl<$($gen:ident)*> $struct_name:ty {$(
        $(#[$attrs:meta])*
        $vis:vis unsafe fn $fn_name:ident(self: NonNull<Self>) -> NonNull<$field_type:ty> {
            &self$(.$field_name:tt)+
        }
    )*}
    ) => {
        impl<$($gen)*> $struct_name {$(
            $(#[$attrs])*
            $vis unsafe fn $fn_name(me: ::core::ptr::NonNull<Self>) -> ::core::ptr::NonNull<$field_type> {
                let me = me.as_ptr();
                let me_u8 = me as *mut u8;

                let field_offset = {
                    let me_ref = &*me;
                    let field_ref_u8 = (&me_ref $(.$field_name)+ ) as *const $field_type as *const u8;
                    field_ref_u8.offset_from(me_u8)
                };

                ::core::ptr::NonNull::new_unchecked(me_u8.offset(field_offset).cast())
            }
        )*}
    };
}

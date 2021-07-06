cfg_trace! {
    #[macro_export]
    #[doc(hidden)]
    macro_rules! instrument_resource {
        (
            pin_project,
            $(#[$meta:meta])*
            // pin project gets confused when this is a `vis`
            // and does not infer the projection visibility correctly
            $visibility:ident struct $struct_name:ident {
                $(
                $(#[$field_attrs:ident])*
                $field_vis:vis $field_name:ident : $field_type:ty
                ),*$(,)+
            }
        ) => {
            pin_project_lite::pin_project! {
                $(#[$meta])*
                $visibility struct $struct_name {
                    __resource_span: tracing::Span,
                    $(
                    $(#[$field_attrs])*
                    $field_vis $field_name : $field_type,
                    )*
                }
            }
        };
        (
            $(#[$meta:meta])*
            $vis:vis struct $struct_name:ident {
                $(
                $(#[$field_attrs:ident])*
                $field_vis:vis $field_name:ident : $field_type:ty
                ),*$(,)+
            }
        ) => {
            $(#[$meta])*
            $vis struct $struct_name {
                __resource_span: tracing::Span,
                $(
                $(#[$field_attrs])*
                $field_vis $field_name : $field_type,
                )*
            }
        }
    }


    #[macro_export]
    #[doc(hidden)]
    macro_rules! new_instrumented_resource {
        (
            $resource_type:ident,
            $struct:ident {
            $($field:ident),* $(,)* // Handle non shorthand initialization
            }
        ) => {
            $struct {
                __resource_span:tracing::trace_span!(
                    "resource",
                    concrete_type = stringify!($struct),
                    kind = stringify!($resource_type)
                ),
                $(
                    $field,
                )*
            }
        };
    }

    #[macro_export]
    #[doc(hidden)]
    macro_rules! instrument_resource_op {
        (
            $( #[$attr:meta] )*
            $vis:vis fn $name:ident(&$self: ident, $($arg_name:ident : $arg_ty:ty),* $(,)*) $(-> $ret:ty)?
            $body:block
        ) => {
            $vis fn $name(&$self, $($arg_name : $arg_ty,)*) $(-> $ret)? {
                let __resource_span = $self.__resource_span.clone();
                let __resource_span_guard = __resource_span.enter();
                let result =  (|| $body)();
                drop(__resource_span_guard);
                result
            }
        };
        (
            $( #[$attr:meta] )*
            $vis:vis fn $name:ident($self:tt : $self_type:ty, $($arg_name:ident : $arg_ty:ty),* $(,)*) $(-> $ret:ty)?
            $body:block
        ) => {
            $vis fn $name($self : $self_type, $($arg_name : $arg_ty,)*) $(-> $ret)? {
                let __resource_span = $self.__resource_span.clone();
                let __resource_span_guard = __resource_span.enter();
                let result =  (|| $body)();
                drop(__resource_span_guard);
                result
            }
        };
    }
}

cfg_not_trace! {
    #[macro_export]
    #[doc(hidden)]
    macro_rules! instrument_resource {
        (pin_project, $($t:tt)*) => {
            pin_project_lite::pin_project! {
                $($t)*
            }
        };
        ($($t:tt)*) => {
            $($t)*
        }
    }

    #[macro_export]
    #[doc(hidden)]
    macro_rules! new_instrumented_resource {
        ($resource_type:ident, $($t:tt)*) => {
            $($t)*
        }
    }

    #[macro_export]
    #[doc(hidden)]
    macro_rules! instrument_resource_op {
        ($($t:tt)*) => {
            $($t)*
        }
    }
}

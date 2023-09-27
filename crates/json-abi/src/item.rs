use crate::{param::Param, utils::*, EventParam, InternalType, StateMutability};
use alloc::{borrow::Cow, string::String, vec::Vec};
use alloy_primitives::{keccak256, Selector, B256};
use alloy_sol_type_parser::{TypeSpecifier, TypeStem};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Serde order:
// Public items -> public enum -> private enum -> private items
//
// Items are duplicated to be able to make use of the derived `serde` impl,
// while enforcing that the public items emit their tag, as per the spec.
//
// They are all declared with `repr(C)` because the default repr (`Rust`) does
// not have any layout guarantees, which we need to be able to transmute between
// the private and public types.
macro_rules! abi_items {
    ($(
        $(#[$attr:meta])*
        $vis:vis struct $name:ident {$(
            $(#[$fattr:meta])*
            $fvis:vis $field:ident : $type:ty,
        )*}
    )*) => {
        $(
            $(#[$attr])*
            #[derive(Clone, Debug, PartialEq, Eq, Hash)]
            #[repr(C)]
            $vis struct $name {$(
                $(#[$fattr])*
                $fvis $field: $type,
            )*}

            impl From<$name> for AbiItem<'_> {
                #[inline]
                fn from(item: $name) -> Self {
                    AbiItem::$name(Cow::Owned(item))
                }
            }

            impl<'a> From<&'a $name> for AbiItem<'a> {
                #[inline]
                fn from(item: &'a $name) -> Self {
                    AbiItem::$name(Cow::Borrowed(item))
                }
            }

            impl<'de> Deserialize<'de> for $name {
                #[inline]
                fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                    AbiItem::deserialize(deserializer).and_then(|item| {
                        if let Some(name) = item.name() {
                            validate_identifier!(name);
                        };
                        match item {
                            AbiItem::$name(item) => Ok(item.into_owned()),
                            item => Err(serde::de::Error::invalid_type(
                                serde::de::Unexpected::Other(item.debug_name()),
                                &stringify!($name),
                            )),
                        }
                    })
                }
            }

            impl Serialize for $name {
                #[inline]
                fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                    AbiItem::$name(Cow::Borrowed(self)).serialize(serializer)
                }
            }
        )*

        /// A JSON ABI item.
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        #[repr(C)]
        pub enum AbiItem<'a> {$(
            #[doc = concat!("A JSON ABI [`", stringify!($name), "`].")]
            $name(Cow<'a, $name>),
        )*}

        #[doc(hidden)]
        mod private {
            use super::*;

            $(
                #[derive(Clone, Serialize, Deserialize)]
                #[serde(rename_all = "camelCase")]
                #[repr(C)]
                pub(super) struct $name {$(
                    $field: $type,
                )*}
            )*

            #[derive(Serialize, Deserialize)]
            #[serde(rename_all = "lowercase", tag = "type")]
            #[repr(C)]
            pub(super) enum AbiItem<'a> {$(
                $name(Cow<'a, self::$name>),
            )*}
        }
    };
}

abi_items! {
    /// A JSON ABI constructor function.
    pub struct Constructor {
        /// The input types of the constructor. May be empty.
        pub inputs: Vec<Param>,
        /// The state mutability of the constructor.
        pub state_mutability: StateMutability,
    }

    /// A JSON ABI fallback function.
    #[derive(Copy)]
    pub struct Fallback {
        /// The state mutability of the fallback function.
        pub state_mutability: StateMutability,
    }

    /// A JSON ABI receive function.
    #[derive(Copy)]
    pub struct Receive {
        /// The state mutability of the receive function.
        pub state_mutability: StateMutability,
    }

    /// A JSON ABI function.
    pub struct Function {
        /// The name of the function.
        pub name: String,
        /// The input types of the function. May be empty.
        pub inputs: Vec<Param>,
        /// The output types of the function. May be empty.
        pub outputs: Vec<Param>,
        /// The state mutability of the function.
        pub state_mutability: StateMutability,
    }

    /// A JSON ABI event.
    pub struct Event {
        /// The name of the event.
        pub name: String,
        /// A list of the event's inputs, in order.
        pub inputs: Vec<EventParam>,
        /// Whether the event is anonymous. Anonymous events do not have their
        /// Signature included in the topic 0. Instead, the indexed arguments
        /// are 0-indexed.
        pub anonymous: bool,
    }

    /// A JSON ABI error.
    pub struct Error {
        /// The name of the error.
        pub name: String,
        /// A list of the error's components, in order.
        pub inputs: Vec<Param>,
    }
}

impl Serialize for AbiItem<'_> {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        <&private::AbiItem<'_>>::from(self).serialize(serializer)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for AbiItem<'a> {
    #[inline]
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        private::AbiItem::deserialize(deserializer).map(Into::into)
    }
}

// SAFETY: `AbiItem` and `private::AbiItem` have the exact same variants, and
// all the items use a non-Rust repr.
// This is enforced in the macro.
#[doc(hidden)]
impl<'a> From<private::AbiItem<'a>> for AbiItem<'a> {
    #[inline(always)]
    fn from(item: private::AbiItem<'a>) -> AbiItem<'a> {
        unsafe { core::mem::transmute(item) }
    }
}

#[doc(hidden)]
impl<'a> From<AbiItem<'a>> for private::AbiItem<'a> {
    #[inline(always)]
    fn from(item: AbiItem<'a>) -> private::AbiItem<'a> {
        unsafe { core::mem::transmute(item) }
    }
}

#[doc(hidden)]
impl<'a, 'r> From<&'r private::AbiItem<'a>> for &'r AbiItem<'a> {
    #[inline(always)]
    fn from(item: &'r private::AbiItem<'a>) -> &'r AbiItem<'a> {
        unsafe { core::mem::transmute(item) }
    }
}

#[doc(hidden)]
impl<'a, 'r> From<&'r AbiItem<'a>> for &'r private::AbiItem<'a> {
    #[inline(always)]
    fn from(item: &'r AbiItem<'a>) -> &'r private::AbiItem<'a> {
        unsafe { core::mem::transmute(item) }
    }
}

impl AbiItem<'_> {
    /// Returns the debug name of the item.
    #[inline]
    pub const fn debug_name(&self) -> &'static str {
        match self {
            AbiItem::Constructor(_) => "Constructor",
            AbiItem::Fallback(_) => "Fallback",
            AbiItem::Receive(_) => "Receive",
            AbiItem::Function(_) => "Function",
            AbiItem::Event(_) => "Event",
            AbiItem::Error(_) => "Error",
        }
    }

    /// Returns an immutable reference to the name of the item.
    #[inline]
    pub fn name(&self) -> Option<&String> {
        match self {
            Self::Event(item) => Some(&item.name),
            Self::Error(item) => Some(&item.name),
            Self::Function(item) => Some(&item.name),
            Self::Constructor(_) | Self::Fallback(_) | Self::Receive(_) => None,
        }
    }

    /// Returns a mutable reference to the name of the item.
    ///
    /// Clones the item if it is not already owned.
    #[inline]
    pub fn name_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Event(item) => Some(&mut item.to_mut().name),
            Self::Error(item) => Some(&mut item.to_mut().name),
            Self::Function(item) => Some(&mut item.to_mut().name),
            Self::Constructor(_) | Self::Fallback(_) | Self::Receive(_) => None,
        }
    }

    /// Returns the state mutability of the item.
    #[inline]
    pub fn state_mutability(&self) -> Option<StateMutability> {
        match self {
            Self::Constructor(item) => Some(item.state_mutability),
            Self::Fallback(item) => Some(item.state_mutability),
            Self::Receive(item) => Some(item.state_mutability),
            Self::Function(item) => Some(item.state_mutability),
            Self::Event(_) | Self::Error(_) => None,
        }
    }

    /// Returns a mutable reference to the state mutability of the item.
    ///
    /// Clones the item if it is not already owned.
    #[inline]
    pub fn state_mutability_mut(&mut self) -> Option<&mut StateMutability> {
        match self {
            Self::Constructor(item) => Some(&mut item.to_mut().state_mutability),
            Self::Fallback(item) => Some(&mut item.to_mut().state_mutability),
            Self::Receive(item) => Some(&mut item.to_mut().state_mutability),
            Self::Function(item) => Some(&mut item.to_mut().state_mutability),
            Self::Event(_) | Self::Error(_) => None,
        }
    }

    /// Returns an immutable reference to the inputs of the item.
    ///
    /// Use [`event_inputs`](Self::event_inputs) for events instead.
    #[inline]
    pub fn inputs(&self) -> Option<&Vec<Param>> {
        match self {
            Self::Error(item) => Some(&item.inputs),
            Self::Constructor(item) => Some(&item.inputs),
            Self::Function(item) => Some(&item.inputs),
            Self::Event(_) | Self::Fallback(_) | Self::Receive(_) => None,
        }
    }

    /// Returns a mutable reference to the inputs of the item.
    ///
    /// Clones the item if it is not already owned.
    ///
    /// Use [`event_inputs`](Self::event_inputs) for events instead.
    #[inline]
    pub fn inputs_mut(&mut self) -> Option<&mut Vec<Param>> {
        match self {
            Self::Error(item) => Some(&mut item.to_mut().inputs),
            Self::Constructor(item) => Some(&mut item.to_mut().inputs),
            Self::Function(item) => Some(&mut item.to_mut().inputs),
            Self::Event(_) | Self::Fallback(_) | Self::Receive(_) => None,
        }
    }

    /// Returns an immutable reference to the event inputs of the item.
    ///
    /// Use [`inputs`](Self::inputs) for other items instead.
    #[inline]
    pub fn event_inputs(&self) -> Option<&Vec<EventParam>> {
        match self {
            Self::Event(item) => Some(&item.inputs),
            Self::Constructor(_)
            | Self::Fallback(_)
            | Self::Receive(_)
            | Self::Error(_)
            | Self::Function(_) => None,
        }
    }

    /// Returns a mutable reference to the event inputs of the item.
    ///
    /// Clones the item if it is not already owned.
    ///
    /// Use [`inputs`](Self::inputs) for other items instead.
    #[inline]
    pub fn event_inputs_mut(&mut self) -> Option<&mut Vec<EventParam>> {
        match self {
            Self::Event(item) => Some(&mut item.to_mut().inputs),
            Self::Constructor(_)
            | Self::Fallback(_)
            | Self::Receive(_)
            | Self::Error(_)
            | Self::Function(_) => None,
        }
    }

    /// Returns an immutable reference to the outputs of the item.
    #[inline]
    pub fn outputs(&self) -> Option<&Vec<Param>> {
        match self {
            Self::Function(item) => Some(&item.outputs),
            Self::Constructor(_)
            | Self::Fallback(_)
            | Self::Receive(_)
            | Self::Error(_)
            | Self::Event(_) => None,
        }
    }

    /// Returns an immutable reference to the outputs of the item.
    #[inline]
    pub fn outputs_mut(&mut self) -> Option<&mut Vec<Param>> {
        match self {
            Self::Function(item) => Some(&mut item.to_mut().outputs),
            Self::Constructor(_)
            | Self::Fallback(_)
            | Self::Receive(_)
            | Self::Error(_)
            | Self::Event(_) => None,
        }
    }
}

impl Error {
    /// Computes this error's signature: `$name($($inputs),*)`.
    ///
    /// This is the preimage input used to [compute the
    /// selector](Self::selector).
    #[inline]
    pub fn signature(&self) -> String {
        signature(&self.name, &self.inputs, None)
    }

    /// Computes this error's selector: `keccak256(self.signature())[..4]`
    #[inline]
    pub fn selector(&self) -> Selector {
        selector(&self.signature())
    }

    /// Parse a `String` into `Self`
    pub fn parse(str: &str) -> Result<Self, String> {
        let open_paren_idx = str
            .find('(')
            .ok_or("No opening parenthesis found".to_string())?;
        let name = str[0..open_paren_idx].to_string();
        let params_str = &str[(open_paren_idx + 1)..str.len() - 1]; // Exclude the last closing parenthesis

        let params = parse_params(params_str)?;

        Ok(Error {
            name,
            inputs: params,
        })
    }
}

fn parse_params(params: &str) -> Result<Vec<Param>, String> {
    let mut result = vec![];
    let mut iter = params.chars().peekable();
    let mut buffer = String::new();
    let mut nesting_level = 0;
    while let Some(ch) = iter.next() {
        match ch {
            '(' => {
                nesting_level += 1;
                buffer.push(ch);
            }
            ')' => {
                nesting_level -= 1;
                buffer.push(ch);
            }
            ',' => {
                if nesting_level == 0 {
                    // This comma is not inside a tuple, so it separates parameters
                    let param = parse_param(&buffer.trim())?;
                    result.push(param);
                    buffer.clear();
                } else {
                    // This comma is inside a tuple, so we don't want to treat it as a parameter
                    // separator
                    buffer.push(ch);
                }
            }
            _ => {
                buffer.push(ch);
            }
        }
    }

    if !buffer.is_empty() {
        let param = parse_param(&buffer.trim())?;
        result.push(param);
    }
    Ok(result)
}

fn parse_param(param_str: &str) -> Result<Param, String> {
    // Assumption: whitespaces only to separate type and name.
    // For example:
    // `uint256 arg1`
    // Never put whitespaces between args like this:
    // `uint256 arg1, uint256 arg2`
    //              ^
    //              |----> this whitespace is not allowed!
    let mut iter = param_str.split(" ");
    let ty_str = iter.next().ok_or("Incorrect format used")?;
    let name = iter.next().ok_or("Incorrect format used")?;

    let stem = TypeSpecifier::parse(ty_str)
        .map_err(|_| "Incorrect format used")?
        .stem;
    let mut components = vec![];
    let ty;
    match stem {
        TypeStem::Root(_) => {
            ty = ty_str.to_string();
        }
        TypeStem::Tuple(tuple_type) => {
            ty = "tuple".to_string();
            if !tuple_type.types.is_empty() {
                for type_specifier in tuple_type.types.iter() {
                    // adding a whitespace in order to handle gracefully the empty name
                    match parse_param((type_specifier.span.to_owned() + " ").as_str()) {
                        Ok(param) => components.push(param),
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    let param = Param {
        name: name.to_string(),
        ty,
        components,
        internal_type: InternalType::parse(ty_str),
    };
    Ok(param)
}

impl Function {
    /// Returns this function's signature: `$name($($inputs),*)`.
    ///
    /// This is the preimage input used to [compute the
    /// selector](Self::selector).
    #[inline]
    pub fn signature(&self) -> String {
        signature(&self.name, &self.inputs, None)
    }

    /// Returns this function's full signature:
    /// `$name($($inputs),*)($(outputs),*)`.
    ///
    /// This is the same as [`signature`](Self::signature), but also includes
    /// the output types.
    #[inline]
    pub fn signature_full(&self) -> String {
        signature(&self.name, &self.inputs, Some(&self.outputs))
    }

    /// Computes this error's selector: `keccak256(self.signature())[..4]`
    #[inline]
    pub fn selector(&self) -> Selector {
        selector(&self.signature())
    }
}

impl Event {
    /// Returns this event's signature: `$name($($inputs),*)`.
    ///
    /// This is the preimage input used to [compute the
    /// selector](Self::selector).
    #[inline]
    pub fn signature(&self) -> String {
        event_signature(&self.name, &self.inputs)
    }

    /// Computes this event's selector: `keccak256(self.signature())`
    #[inline]
    pub fn selector(&self) -> B256 {
        keccak256(self.signature().as_bytes())
    }
}

#[cfg(test)]
mod test {
    use crate::Error;
    #[test]
    fn test1() {
        let error_str = "Myerror(uint256 a,(address,uint256) arg2)";
        let err = Error::parse(error_str).unwrap();
        println!("{:#?}", err);
    }

    #[test]
    fn test2() {
        let error_str = "Myerror((address,uint256) arg2)";
        let err = Error::parse(error_str).unwrap();
        println!("{:#?}", err);
    }

    #[test]
    fn test3() {
        let error_str =
            "Myerror(uint256 a,(address,uint256) arg2,(address,uint256,(uint256,uint256[2])) arg3)";
        let err = Error::parse(error_str).unwrap();
        println!("{:#?}", err);
    }

    #[test]
    fn test4() {
        let error_str = "Myerror((address,(uint256,uint256[2])) arg3)";
        let err = Error::parse(error_str).unwrap();
        println!("{:#?}", err);
    }
}

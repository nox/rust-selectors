/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::ascii::AsciiExt;
use std::borrow::Cow;
use std::convert::{From, Into};
use std::default::Default;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;
#[cfg(feature = "heap_size")]
use heapsize::HeapSizeOf;

use cssparser::{Token, Parser, parse_nth};
use string_cache::{Atom, Namespace};

use hash_map;
use specificity::UnpackedSpecificity;
pub use specificity::Specificity;

/// This trait allows to define the parser implementation in regards
/// of pseudo-classes/elements
pub trait SelectorImpl {
    /// non tree-structural pseudo-classes
    /// (see: https://drafts.csswg.org/selectors/#structural-pseudos)
    #[cfg(feature = "heap_size")]
    type NonTSPseudoClass: Clone + Debug + Eq + Hash + HeapSizeOf + PartialEq + Sized;
    #[cfg(not(feature = "heap_size"))]
    type NonTSPseudoClass: Clone + Debug + Eq + Hash + PartialEq + Sized;

    /// This function can return an "Err" pseudo-element in order to support CSS2.1
    /// pseudo-elements.
    fn parse_non_ts_pseudo_class(_context: &ParserContext,
                                 _name: &str)
        -> Result<Self::NonTSPseudoClass, ()> { Err(()) }

    /// pseudo-elements
    #[cfg(feature = "heap_size")]
    type PseudoElement: Sized + PartialEq + Eq + Clone + Debug + Hash + HeapSizeOf;
    #[cfg(not(feature = "heap_size"))]
    type PseudoElement: Sized + PartialEq + Eq + Clone + Debug + Hash;
    fn parse_pseudo_element(_context: &ParserContext,
                            _name: &str)
        -> Result<Self::PseudoElement, ()> { Err(()) }
}

pub struct ParserContext {
    pub in_user_agent_stylesheet: bool,
    pub default_namespace: Option<Namespace>,
    pub namespace_prefixes: hash_map::HashMap<String, Namespace>,
}

impl ParserContext {
    pub fn new() -> ParserContext {
        ParserContext {
            in_user_agent_stylesheet: false,
            default_namespace: None,
            namespace_prefixes: hash_map::new(),
        }
    }
}

#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
#[derive(PartialEq, Clone, Debug)]
pub struct Selector<Impl: SelectorImpl> {
    pub complex_selector: Arc<ComplexSelector<Impl>>,
    pub pseudo_element: Option<Impl::PseudoElement>,
    pub specificity: Specificity,
}

#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ComplexSelector<Impl: SelectorImpl> {
    pub compound_selector: Box<[SimpleSelector<Impl>]>,
    pub next: Option<(Arc<ComplexSelector<Impl>>, Combinator)>,  // c.next is left of c
}

#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Combinator {
    Child,  //  >
    Descendant,  // space
    NextSibling,  // +
    LaterSibling,  // ~
}

#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SimpleSelector<Impl: SelectorImpl> {
    ID(Atom),
    Class(Atom),
    LocalName(LocalName),
    Namespace(Namespace),

    // Attribute selectors
    AttrExists(AttrSelector),  // [foo]
    AttrEqual(AttrSelector, String, CaseSensitivity),  // [foo=bar]
    AttrIncludes(AttrSelector, String),  // [foo~=bar]
    AttrDashMatch(AttrSelector, String, String), // [foo|=bar]  Second string is the first + "-"
    AttrPrefixMatch(AttrSelector, String),  // [foo^=bar]
    AttrSubstringMatch(AttrSelector, String),  // [foo*=bar]
    AttrSuffixMatch(AttrSelector, String),  // [foo$=bar]

    // Pseudo-classes
    Negation(Box<[Arc<ComplexSelector<Impl>>]>),
    FirstChild, LastChild, OnlyChild,
    Root,
    Empty,
    NthChild(i32, i32),
    NthLastChild(i32, i32),
    NthOfType(i32, i32),
    NthLastOfType(i32, i32),
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NonTSPseudoClass(Impl::NonTSPseudoClass),
    // ...
}


#[derive(Eq, PartialEq, Clone, Hash, Copy, Debug)]
#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
pub enum CaseSensitivity {
    CaseSensitive,  // Selectors spec says language-defined, but HTML says sensitive.
    CaseInsensitive,
}


#[derive(Eq, PartialEq, Clone, Hash, Debug)]
#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
pub struct LocalName {
    pub name: Atom,
    pub lower_name: Atom,
}

#[derive(Eq, PartialEq, Clone, Hash, Debug)]
#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
pub struct AttrSelector {
    pub name: Atom,
    pub lower_name: Atom,
    pub namespace: NamespaceConstraint,
}

#[derive(Eq, PartialEq, Clone, Hash, Debug)]
#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
pub enum NamespaceConstraint {
    Any,
    Specific(Namespace),
}

fn specificity<Impl>(complex_selector: &ComplexSelector<Impl>,
                     pseudo_element: Option<&Impl::PseudoElement>)
                     -> Specificity
				     where Impl: SelectorImpl {
    let mut specificity = complex_selector_specificity(complex_selector);
    if pseudo_element.is_some() {
        specificity.element_selectors += 1;
    }
    specificity.into()
}

fn complex_selector_specificity<Impl>(mut selector: &ComplexSelector<Impl>)
                                      -> UnpackedSpecificity
                                      where Impl: SelectorImpl {
    fn compound_selector_specificity<Impl>(compound_selector: &[SimpleSelector<Impl>],
                                           specificity: &mut UnpackedSpecificity)
                                           where Impl: SelectorImpl {
        for simple_selector in compound_selector.iter() {
            match *simple_selector {
                SimpleSelector::LocalName(..) =>
                    specificity.element_selectors += 1,
                SimpleSelector::ID(..) =>
                    specificity.id_selectors += 1,
                SimpleSelector::Class(..) |
                SimpleSelector::AttrExists(..) |
                SimpleSelector::AttrEqual(..) |
                SimpleSelector::AttrIncludes(..) |
                SimpleSelector::AttrDashMatch(..) |
                SimpleSelector::AttrPrefixMatch(..) |
                SimpleSelector::AttrSubstringMatch(..) |
                SimpleSelector::AttrSuffixMatch(..) |

                SimpleSelector::FirstChild | SimpleSelector::LastChild |
                SimpleSelector::OnlyChild | SimpleSelector::Root |
                SimpleSelector::Empty |
                SimpleSelector::NthChild(..) |
                SimpleSelector::NthLastChild(..) |
                SimpleSelector::NthOfType(..) |
                SimpleSelector::NthLastOfType(..) |
                SimpleSelector::FirstOfType | SimpleSelector::LastOfType |
                SimpleSelector::OnlyOfType |
                SimpleSelector::NonTSPseudoClass(..) =>
                    specificity.class_like_selectors += 1,
                SimpleSelector::Namespace(..) => (),
                SimpleSelector::Negation(ref negated) => {
                    let negated_specificities =
                        negated.iter().map(|sel| complex_selector_specificity(sel));
                    *specificity = *specificity + negated_specificities.max().unwrap();
                }
            }
        }
    }

    let mut specificity = Default::default();
    compound_selector_specificity(&selector.compound_selector,
                              &mut specificity);
    loop {
        match selector.next {
            None => break,
            Some((ref next_selector, _)) => {
                selector = &**next_selector;
                compound_selector_specificity(&selector.compound_selector,
                                          &mut specificity)
            }
        }
    }
    specificity
}



pub fn parse_author_origin_selector_list_from_str<Impl>(input: &str)
                                                        -> Result<Box<[Selector<Impl>]>, ()>
                                                        where Impl: SelectorImpl {
    let context = ParserContext::new();
    parse_selector_list(&context, &mut Parser::new(input))
}

/// Parse a selector list.
///
/// * `Err(())` invalid selector list, abort.
pub fn parse_selector_list<Impl>(context: &ParserContext, input: &mut Parser)
                                 -> Result<Box<[Selector<Impl>]>, ()>
                                 where Impl: SelectorImpl {
    input.parse_comma_separated(|input| parse_selector(context, input)).map(Vec::into_boxed_slice)
}


/// Parse a selector.
///
/// * `Err(())`: invalid selector, abort.
fn parse_selector<Impl>(context: &ParserContext, input: &mut Parser)
                        -> Result<Selector<Impl>, ()>
                        where Impl: SelectorImpl {
    let complex =
        try!(parse_complex_selector::<Impl>(context, input));
    let pseudo_element = try!(parse_pseudo_element::<Impl>(context, input));
    if !complex.compound_selector.is_empty() || pseudo_element.is_some() {
        let specificity = specificity(&complex, pseudo_element.as_ref());
        Ok(Selector {
            complex_selector: Arc::new(complex),
            pseudo_element: pseudo_element,
            specificity: specificity,
        })
    } else {
        Err(())
    }
}

/// Parse a complex selector.
///
/// Its first compound selector might be empty, in which case `next` should
/// be null and caller should look for a pseudo-element selector or abort.
///
/// * `Err(())`: invalid complex selector, abort.
fn parse_complex_selector<Impl>(context: &ParserContext, input: &mut Parser)
                                -> Result<ComplexSelector<Impl>, ()>
                                where Impl: SelectorImpl {
    skip_whitespace(input);
    let compound =
        try!(parse_compound_selector::<Impl>(context, input));
    let mut complex = ComplexSelector { compound_selector: compound, next: None };
    if complex.compound_selector.is_empty() {
        return Ok(complex);
    }
    'outer_loop: loop {
        let combinator;
        let mut any_whitespace = false;
        let mut position;
        loop {
            position = input.position();
            match input.next_including_whitespace() {
                Err(()) => break 'outer_loop,
                Ok(Token::WhiteSpace(_)) => any_whitespace = true,
                Ok(Token::Delim('>')) => {
                    combinator = Combinator::Child;
                    break
                }
                Ok(Token::Delim('+')) => {
                    combinator = Combinator::NextSibling;
                    break
                }
                Ok(Token::Delim('~')) => {
                    combinator = Combinator::LaterSibling;
                    break
                }
                Ok(_) => {
                    input.reset(position);
                    if any_whitespace {
                        combinator = Combinator::Descendant;
                        break
                    } else {
                        break 'outer_loop
                    }
                }
            }
        }
        if combinator != Combinator::Descendant {
            skip_whitespace(input);
        }
        let compound =
            try!(parse_compound_selector::<Impl>(context, input));
        complex = ComplexSelector {
            compound_selector: compound,
            next: Some((Arc::new(complex), combinator)),
        };
        if complex.compound_selector.is_empty() {
            break;
        }
    }
    Ok(complex)
}

/// Parse a compound selector.
///
/// If there is any type selector or universal selector, it is the first one.
///
/// [ type_selector | universal ]? [ HASH | class | attrib | negation ]+
///
/// * `Err(())`: Invalid sequence, abort.
fn parse_compound_selector<Impl>(context: &ParserContext,
                                 input: &mut Parser)
                                 -> Result<Box<[SimpleSelector<Impl>]>, ()>
                                 where Impl: SelectorImpl {
    let mut compound_selector =
        try!(parse_type_selector::<Impl>(context, input)).unwrap_or(vec![]);
    loop {
        match try!(parse_one_simple_selector::<Impl>(context, input)) {
            None => break,
            Some(s) => compound_selector.push(s),
        }
    }
    Ok(compound_selector.into_boxed_slice())
}

/// * `Err(())`: Invalid selector, abort
/// * `Ok(None)`: Not a type selector, could be something else. `input` was not consumed.
/// * `Ok(Some(vec))`: Length 0 (`*|*`), 1 (`*|E` or `ns|*`) or 2 (`|E` or `ns|E`)
fn parse_type_selector<Impl: SelectorImpl>(context: &ParserContext, input: &mut Parser)
                       -> Result<Option<Vec<SimpleSelector<Impl>>>, ()> {
    match try!(parse_qualified_name(context, input, /* in_attr_selector = */ false)) {
        None => Ok(None),
        Some((namespace, local_name)) => {
            let mut compound_selector = vec!();
            match namespace {
                NamespaceConstraint::Specific(ns) => {
                    compound_selector.push(SimpleSelector::Namespace(ns))
                },
                NamespaceConstraint::Any => (),
            }
            match local_name {
                Some(name) => {
                    compound_selector.push(SimpleSelector::LocalName(LocalName {
                        name: Atom::from(&*name),
                        lower_name: Atom::from(&*name.to_ascii_lowercase()),
                    }))
                }
                None => (),
            }
            Ok(Some(compound_selector))
        }
    }
}

/// * `Err(())`: Invalid selector, abort
/// * `Ok(None)`: Not a simple selector, could be something else. `input` was not consumed.
/// * `Ok(Some((namespace, local_name)))`: `None` for the local name means a `*` universal selector
fn parse_qualified_name<'i, 't>
                       (context: &ParserContext, input: &mut Parser<'i, 't>,
                        in_attr_selector: bool)
                        -> Result<Option<(NamespaceConstraint, Option<Cow<'i, str>>)>, ()> {
    let default_namespace = |local_name| {
        let namespace = match context.default_namespace {
            Some(ref ns) => NamespaceConstraint::Specific(ns.clone()),
            None => NamespaceConstraint::Any,
        };
        Ok(Some((namespace, local_name)))
    };

    let explicit_namespace = |input: &mut Parser<'i, 't>, namespace| {
        match input.next_including_whitespace() {
            Ok(Token::Delim('*')) if !in_attr_selector => {
                Ok(Some((namespace, None)))
            },
            Ok(Token::Ident(local_name)) => {
                Ok(Some((namespace, Some(local_name))))
            },
            _ => Err(()),
        }
    };

    let position = input.position();
    match input.next_including_whitespace() {
        Ok(Token::Ident(value)) => {
            let position = input.position();
            match input.next_including_whitespace() {
                Ok(Token::Delim('|')) => {
                    let result = context.namespace_prefixes.get(&*value);
                    let namespace = try!(result.ok_or(()));
                    explicit_namespace(input, NamespaceConstraint::Specific(namespace.clone()))
                },
                _ => {
                    input.reset(position);
                    if in_attr_selector {
                        Ok(Some((NamespaceConstraint::Specific(ns!()), Some(value))))
                    } else {
                        default_namespace(Some(value))
                    }
                }
            }
        },
        Ok(Token::Delim('*')) => {
            let position = input.position();
            match input.next_including_whitespace() {
                Ok(Token::Delim('|')) => explicit_namespace(input, NamespaceConstraint::Any),
                _ => {
                    input.reset(position);
                    if in_attr_selector {
                        Err(())
                    } else {
                        default_namespace(None)
                    }
                },
            }
        },
        Ok(Token::Delim('|')) => explicit_namespace(input, NamespaceConstraint::Specific(ns!())),
        _ => {
            input.reset(position);
            Ok(None)
        }
    }
}


fn parse_attribute_selector<Impl: SelectorImpl>(context: &ParserContext, input: &mut Parser)
                            -> Result<SimpleSelector<Impl>, ()> {
    let attr = match try!(parse_qualified_name(context, input, /* in_attr_selector = */ true)) {
        None => return Err(()),
        Some((_, None)) => unreachable!(),
        Some((namespace, Some(local_name))) => AttrSelector {
            namespace: namespace,
            lower_name: Atom::from(&*local_name.to_ascii_lowercase()),
            name: Atom::from(&*local_name),
        },
    };

    fn parse_value(input: &mut Parser) -> Result<String, ()> {
        Ok((try!(input.expect_ident_or_string())).into_owned())
    }
    // TODO: deal with empty value or value containing whitespace (see spec)
    match input.next() {
        // [foo]
        Err(()) => Ok(SimpleSelector::AttrExists(attr)),

        // [foo=bar]
        Ok(Token::Delim('=')) => {
            Ok(SimpleSelector::AttrEqual(attr, try!(parse_value(input)),
                                         try!(parse_attribute_flags(input))))
        }
        // [foo~=bar]
        Ok(Token::IncludeMatch) => {
            Ok(SimpleSelector::AttrIncludes(attr, try!(parse_value(input))))
        }
        // [foo|=bar]
        Ok(Token::DashMatch) => {
            let value = try!(parse_value(input));
            let dashing_value = format!("{}-", value);
            Ok(SimpleSelector::AttrDashMatch(attr, value, dashing_value))
        }
        // [foo^=bar]
        Ok(Token::PrefixMatch) => {
            Ok(SimpleSelector::AttrPrefixMatch(attr, try!(parse_value(input))))
        }
        // [foo*=bar]
        Ok(Token::SubstringMatch) => {
            Ok(SimpleSelector::AttrSubstringMatch(attr, try!(parse_value(input))))
        }
        // [foo$=bar]
        Ok(Token::SuffixMatch) => {
            Ok(SimpleSelector::AttrSuffixMatch(attr, try!(parse_value(input))))
        }
        _ => Err(())
    }
}


fn parse_attribute_flags(input: &mut Parser) -> Result<CaseSensitivity, ()> {
    match input.next() {
        Err(()) => Ok(CaseSensitivity::CaseSensitive),
        Ok(Token::Ident(ref value)) if value.eq_ignore_ascii_case("i") => {
            Ok(CaseSensitivity::CaseInsensitive)
        }
        _ => Err(())
    }
}

fn parse_negation<Impl: SelectorImpl>(context: &ParserContext, input: &mut Parser)
                                      -> Result<SimpleSelector<Impl>, ()> {
    input.parse_comma_separated(|input| parse_complex_selector(context, input).map(Arc::new))
         .map(Vec::into_boxed_slice)
         .map(SimpleSelector::Negation)
}

fn parse_functional_pseudo_class<Impl>(context: &ParserContext,
                                       input: &mut Parser,
                                       name: &str)
                                       -> Result<SimpleSelector<Impl>, ()>
                                       where Impl: SelectorImpl {
    match_ignore_ascii_case! { name,
        "nth-child" => parse_nth_pseudo_class(input, SimpleSelector::NthChild),
        "nth-of-type" => parse_nth_pseudo_class(input, SimpleSelector::NthOfType),
        "nth-last-child" => parse_nth_pseudo_class(input, SimpleSelector::NthLastChild),
        "nth-last-of-type" => parse_nth_pseudo_class(input, SimpleSelector::NthLastOfType),
        "not" => parse_negation(context, input),
        _ => Err(())
    }
}


fn parse_nth_pseudo_class<Impl: SelectorImpl, F>(input: &mut Parser, selector: F) -> Result<SimpleSelector<Impl>, ()>
where F: FnOnce(i32, i32) -> SimpleSelector<Impl> {
    let (a, b) = try!(parse_nth(input));
    Ok(selector(a, b))
}


/// Parse a simple selector other than a type selector.
///
/// * `Err(())`: Invalid selector, abort.
/// * `Ok(None)`: Not a simple selector, could be something else; `input` was not consumed.
/// * `Ok(Some(_))`: Parsed a simple selector.
fn parse_one_simple_selector<Impl>(context: &ParserContext, input: &mut Parser)
                                   -> Result<Option<SimpleSelector<Impl>>, ()>
                                   where Impl: SelectorImpl {
    let start_position = input.position();
    match input.next_including_whitespace() {
        Ok(Token::IDHash(id)) => {
            Ok(Some(SimpleSelector::ID(Atom::from(&*id))))
        }
        Ok(Token::Delim('.')) => {
            match input.next_including_whitespace() {
                Ok(Token::Ident(class)) => {
                    Ok(Some(SimpleSelector::Class(Atom::from(&*class))))
                }
                _ => Err(()),
            }
        }
        Ok(Token::SquareBracketBlock) => {
            let attr = try!(input.parse_nested_block(|input| {
                parse_attribute_selector(context, input)
            }));
            Ok(Some(attr))
        }
        Ok(Token::Colon) => {
            match input.next_including_whitespace() {
                Ok(Token::Ident(name)) => {
                    match parse_simple_pseudo_class(context, &name) {
                        Ok(pseudo_class) => Ok(Some(pseudo_class)),
                        Err(()) => {
                            // Errors could be CSS 2.1 pseudo-elements.
                            input.reset(start_position);
                            Ok(None)
                        },
                    }
                }
                Ok(Token::Function(name)) => {
                    let pseudo = try!(input.parse_nested_block(|input| {
                        parse_functional_pseudo_class(context, input, &name)
                    }));
                    Ok(Some(pseudo))
                }
                Ok(Token::Colon) => {
                    // Could be a pseudo-element.
                    input.reset(start_position);
                    Ok(None)
                }
                _ => Err(())
            }
        }
        _ => {
            input.reset(start_position);
            Ok(None)
        }
    }
}

fn parse_simple_pseudo_class<Impl: SelectorImpl>(context: &ParserContext, name: &str) -> Result<SimpleSelector<Impl>, ()> {
    match_ignore_ascii_case! { name,
        "first-child" => Ok(SimpleSelector::FirstChild),
        "last-child"  => Ok(SimpleSelector::LastChild),
        "only-child"  => Ok(SimpleSelector::OnlyChild),
        "root" => Ok(SimpleSelector::Root),
        "empty" => Ok(SimpleSelector::Empty),
        "first-of-type" => Ok(SimpleSelector::FirstOfType),
        "last-of-type"  => Ok(SimpleSelector::LastOfType),
        "only-of-type"  => Ok(SimpleSelector::OnlyOfType),
        _ => Impl::parse_non_ts_pseudo_class(context, name).map(|pc| SimpleSelector::NonTSPseudoClass(pc))
    }
}

/// Parse a pseudo-element.
///
/// * `Err(())`: Invalid pseudo-element, abort.
/// * `Ok(None)`: Not a pseudo-element, could be something else; `input` was not consumed.
/// * `Ok(Some(_))`: Parsed a pseudo-element.
fn parse_pseudo_element<Impl>(context: &ParserContext, input: &mut Parser)
                              -> Result<Option<Impl::PseudoElement>, ()>
                              where Impl: SelectorImpl {
    let start_position = input.position();
    if input.next_including_whitespace() != Ok(Token::Colon) {
        input.reset(start_position);
        return Ok(None);
    }
    let name = match input.next_including_whitespace() {
        Ok(Token::Ident(name)) => {
            if is_legacy_pseudo_element(&name) {
                // CSS 2.1 pseudo-element.
                name
            } else {
                return Err(());
            }
        },
        Ok(Token::Colon) => {
            if let Ok(Token::Ident(name)) = input.next_including_whitespace() {
                name
            } else {
                return Err(());
            }
        },
        _ => return Err(()),
    };
    Impl::parse_pseudo_element(context, &name).map(Some)
}

fn is_legacy_pseudo_element(name: &str) -> bool {
    match_ignore_ascii_case! { name,
        "before" => true,
        "after" => true,
        "first-line" => true,
        "first-letter" => true,
        _ => false
    }
}

fn skip_whitespace(input: &mut Parser) {
    loop {
        let position = input.position();
        if !matches!(input.next_including_whitespace(), Ok(Token::WhiteSpace(_))) {
            input.reset(position);
            break
        }
    }
}

// NB: pub module in order to access the DummySelectorImpl
#[cfg(test)]
pub mod tests {
    use std::sync::Arc;
    use cssparser::Parser;
    use specificity::UnpackedSpecificity;
    use string_cache::Atom;
    use super::*;

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub enum PseudoClass {
        ServoNonZeroBorder,
    }

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub enum PseudoElement {
        Before,
        After,
    }

    #[derive(PartialEq, Debug)]
    pub struct DummySelectorImpl;

    impl SelectorImpl for DummySelectorImpl {
        type NonTSPseudoClass = PseudoClass;
        fn parse_non_ts_pseudo_class(context: &ParserContext, name: &str) -> Result<PseudoClass, ()> {
            match_ignore_ascii_case! { name,
                "-servo-nonzero-border" => {
                    if context.in_user_agent_stylesheet {
                        Ok(PseudoClass::ServoNonZeroBorder)
                    } else {
                        Err(())
                    }
                },
                _ => Err(())
            }
        }

        type PseudoElement = PseudoElement;
        fn parse_pseudo_element(_context: &ParserContext, name: &str) -> Result<PseudoElement, ()> {
            match_ignore_ascii_case! { name,
                "before" => Ok(PseudoElement::Before),
                "after" => Ok(PseudoElement::After),
                _ => Err(())
            }
        }
    }

    fn parse(input: &str) -> Result<Box<[Selector<DummySelectorImpl>]>, ()> {
        parse_ns(input, &ParserContext::new())
    }

    fn parse_ns(input: &str, context: &ParserContext) -> Result<Box<[Selector<DummySelectorImpl>]>, ()> {
        parse_selector_list(context, &mut Parser::new(input))
    }

    #[test]
    fn test_empty() {
        let list = parse_author_origin_selector_list_from_str::<DummySelectorImpl>(":empty");
        assert!(list.is_ok());
    }

    #[test]
    fn test_parsing() {
        assert_eq!(parse(""), Err(())) ;
        assert_eq!(parse("EeÉ"), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([SimpleSelector::LocalName(LocalName {
                    name: Atom::from("EeÉ"),
                    lower_name: Atom::from("eeÉ"),
                })]),
                next: None,
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(0, 0, 1).into(),
        }).into_boxed_slice()));
        assert_eq!(parse(".foo"), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([SimpleSelector::Class(Atom::from("foo"))]),
                next: None,
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(0, 1, 0).into(),
        }).into_boxed_slice()));
        assert_eq!(parse("#bar"), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([SimpleSelector::ID(Atom::from("bar"))]),
                next: None,
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(1, 0, 0).into(),
        }).into_boxed_slice()));
        assert_eq!(parse("e.foo#bar"), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([
                    SimpleSelector::LocalName(LocalName {
                        name: Atom::from("e"),
                        lower_name: Atom::from("e")
                    }),
                    SimpleSelector::Class(Atom::from("foo")),
                    SimpleSelector::ID(Atom::from("bar"))
                ]),
                next: None,
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(1, 1, 1).into(),
        }).into_boxed_slice()));
        assert_eq!(parse("e.foo #bar"), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector:
                    Box::new([SimpleSelector::ID(Atom::from("bar"))]),
                next: Some((Arc::new(ComplexSelector {
                    compound_selector: Box::new([
                        SimpleSelector::LocalName(LocalName {
                            name: Atom::from("e"),
                            lower_name: Atom::from("e")
                        }),
                        SimpleSelector::Class(Atom::from("foo"))
                    ]),
                    next: None,
                }), Combinator::Descendant)),
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(1, 1, 1).into(),
        }).into_boxed_slice()));
        // Default namespace does not apply to attribute selectors
        // https://github.com/mozilla/servo/pull/1652
        let mut context = ParserContext::new();
        assert_eq!(parse_ns("[Foo]", &context), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([SimpleSelector::AttrExists(AttrSelector {
                    name: Atom::from("Foo"),
                    lower_name: Atom::from("foo"),
                    namespace: NamespaceConstraint::Specific(ns!()),
                })]),
                next: None,
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(0, 1, 0).into(),
        }).into_boxed_slice()));
        // Default namespace does not apply to attribute selectors
        // https://github.com/mozilla/servo/pull/1652
        context.default_namespace = Some(ns!(mathml));
        assert_eq!(parse_ns("[Foo]", &context), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([SimpleSelector::AttrExists(AttrSelector {
                    name: Atom::from("Foo"),
                    lower_name: Atom::from("foo"),
                    namespace: NamespaceConstraint::Specific(ns!()),
                })]),
                next: None,
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(0, 1, 0).into(),
        }).into_boxed_slice()));
        // Default namespace does apply to type selectors
        assert_eq!(parse_ns("e", &context), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([
                    SimpleSelector::Namespace(ns!(mathml)),
                    SimpleSelector::LocalName(LocalName {
                        name: Atom::from("e"),
                        lower_name: Atom::from("e") }),
                ]),
                next: None,
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(0, 0, 1).into(),
        }).into_boxed_slice()));
        assert_eq!(parse("[attr|=\"foo\"]"), Ok(vec![Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([
                    SimpleSelector::AttrDashMatch(AttrSelector {
                        name: Atom::from("attr"),
                        lower_name: Atom::from("attr"),
                        namespace: NamespaceConstraint::Specific(ns!()),
                    }, "foo".to_owned(), "foo-".to_owned())
                ]),
                next: None,
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(0, 1, 0).into(),
        }].into_boxed_slice()));
        // https://github.com/mozilla/servo/issues/1723
        assert_eq!(parse("::before"), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([]),
                next: None,
            }),
            pseudo_element: Some(PseudoElement::Before),
            specificity: UnpackedSpecificity::new(0, 0, 1).into(),
        }).into_boxed_slice()));
        assert_eq!(parse("div :after"), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([]),
                next: Some((Arc::new(ComplexSelector {
                    compound_selector: Box::new([SimpleSelector::LocalName(LocalName {
                        name: atom!("div"),
                        lower_name: atom!("div")
                    })]),
                    next: None,
                }), Combinator::Descendant)),
            }),
            pseudo_element: Some(PseudoElement::After),
            specificity: UnpackedSpecificity::new(0, 0, 2).into(),
        }).into_boxed_slice()));
        assert_eq!(parse("#d1 > .ok"), Ok(vec![Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([
                    SimpleSelector::Class(Atom::from("ok")),
                ]),
                next: Some((Arc::new(ComplexSelector {
                    compound_selector: Box::new([
                        SimpleSelector::ID(Atom::from("d1")),
                    ]),
                    next: None,
                }), Combinator::Child)),
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(1, 1, 0).into(),
        }].into_boxed_slice()));
        assert_eq!(parse(":not(.babybel, .provel)"), Ok(vec!(Selector {
            complex_selector: Arc::new(ComplexSelector {
                compound_selector: Box::new([SimpleSelector::Negation(
                    Box::new([
                        Arc::new(ComplexSelector {
                            compound_selector:
                                Box::new([SimpleSelector::Class(Atom::from("babybel"))]),
                            next: None
                        }),
                        Arc::new(ComplexSelector {
                            compound_selector:
                                Box::new([SimpleSelector::Class(Atom::from("provel"))]),
                            next: None
                        }),
                    ])
                )]),
                next: None,
            }),
            pseudo_element: None,
            specificity: UnpackedSpecificity::new(0, 1, 0).into(),
        }).into_boxed_slice()));
    }
}

use std::mem;

use hyper::Uri;
use serde::{Deserialize, Serialize};

pub trait Mapper<T> {
    fn map(&self, value: T) -> T;
    fn when<P>(self, predicate: P) -> When<P, Self>
    where
        Self: Sized,
    {
        When::new(predicate, self)
    }
    fn then<M>(self, other: M) -> Then<Self, M>
    where
        Self: Sized,
    {
        Then::new(self, other)
    }
}

pub struct MapFn<F> {
    pub f: F,
}

impl<F, T> Mapper<T> for MapFn<F>
where
    F: Fn(T) -> T,
{
    fn map(&self, value: T) -> T {
        (self.f)(value)
    }
}

pub struct When<P, M> {
    map: M,
    predicate: P,
}

impl<P, M> When<P, M> {
    pub fn new(predicate: P, map: M) -> Self {
        Self { map, predicate }
    }
}

impl<P, M, T> Mapper<T> for When<P, M>
where
    P: Fn(&T) -> bool,
    M: Mapper<T>,
{
    fn map(&self, value: T) -> T {
        if (self.predicate)(&value) {
            self.map.map(value)
        } else {
            value
        }
    }
}

pub struct Then<M, THAN> {
    m: M,
    then: THAN,
}

impl<M, THAN> Then<M, THAN> {
    pub fn new(m: M, then: THAN) -> Self {
        Self { m, then }
    }
}

impl<M, THEN, T> Mapper<T> for Then<M, THEN>
where
    M: Mapper<T>,
    THEN: Mapper<T>,
{
    fn map(&self, value: T) -> T {
        self.then.map(self.m.map(value))
    }
}

pub struct ReplaceFullPath<'a> {
    pub replace: &'a str,
}

// pub struct ReplacePrefixMatch<'a> {
//     pub matched: &'a str,
//     pub replace: &'a str,
// }

impl Mapper<Uri> for ReplaceFullPath<'_> {
    fn map(&self, uri: Uri) -> Uri {
        let mut parts = uri.into_parts();
        let Some(pq) = parts.path_and_query else {
            return Uri::from_parts(parts).expect("should be valid uri");
        };
        let query = pq.query();
        let pnq = match query {
            Some(q) => format!("{}?{}", self.replace, q),
            None => self.replace.to_string(),
        };
        let pnq = hyper::http::uri::PathAndQuery::from_maybe_shared(pnq).expect("should be valid pnq");
        parts.path_and_query = Some(pnq);
        Uri::from_parts(parts).expect("should be valid uri")
    }
}

// impl Mapper for ReplacePrefixMatch<'_> {
//     fn map(&self, uri: Uri) -> Uri {
//         let mut parts = uri.into_parts();
//         let Some(pq) = parts.path_and_query else {
//             return Uri::from_parts(parts).expect("should be valid uri");
//         };
//         let query = pq.query();
//         let path = pq.path();
//         let pnq = if let Some(rest_path) = path.strip_prefix(self.matched) {
//             match query {
//                 Some(q) => format!("{}{}?{}", self.replace, path, q),
//                 None => format!("{}{}", self.replace, path),
//             }
//         } else {
//             match query {
//                 Some(q) => format!("{}?{}", path, q),
//                 None => path.to_string(),
//             }
//         };
//         let pnq = hyper::http::uri::PathAndQuery::from_maybe_shared(pnq).expect("should be valid pnq");
//         parts.path_and_query = Some(pnq);
//         Uri::from_parts(parts).expect("should be valid uri")
//     }
// }

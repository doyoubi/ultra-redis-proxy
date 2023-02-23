use crate::protocol::{Array, BulkStr, Resp};
use crate::protocol::{BinSafeStr, RespVec};
use std::cmp::min;

pub trait ThreadSafe: Send + Sync + 'static {}

impl<T: Send + Sync + 'static> ThreadSafe for T {}

pub fn get_command_element<T: AsRef<[u8]>>(resp: &Resp<T>, index: usize) -> Option<&[u8]> {
    match resp {
        Resp::Arr(Array::Arr(ref resps)) => resps.get(index).and_then(|resp| match resp {
            Resp::Bulk(BulkStr::Str(s)) => Some(s.as_ref()),
            _ => None,
        }),
        _ => None,
    }
}

pub fn get_command_len<T>(resp: &Resp<T>) -> Option<usize> {
    match resp {
        Resp::Arr(Array::Arr(ref resps)) => Some(resps.len()),
        _ => None,
    }
}

pub fn change_bulk_array_element(resp: &mut RespVec, index: usize, data: Vec<u8>) -> bool {
    match resp {
        Resp::Arr(Array::Arr(ref mut resps)) => {
            Some(true) == resps.get_mut(index).map(|resp| change_bulk_str(resp, data))
        }
        _ => false,
    }
}

pub fn left_trim_array<T>(resp: &mut Resp<T>, removed_num: usize) -> Option<usize> {
    match resp {
        Resp::Arr(Array::Arr(ref mut resps)) => {
            let start = min(removed_num, resps.len());
            let new_resps = resps.drain(start..).collect();
            *resps = new_resps;
            Some(resps.len())
        }
        _ => None,
    }
}

// Returns success or not
pub fn array_append_front(resp: &mut RespVec, preceding_elements: Vec<BinSafeStr>) -> bool {
    match resp {
        Resp::Arr(Array::Arr(ref mut resps)) => {
            let mut new_resps: Vec<_> = preceding_elements
                .into_iter()
                .map(|s| Resp::Bulk(BulkStr::Str(s)))
                .collect();
            new_resps.append(resps);
            *resps = new_resps;
            true
        }
        _ => false,
    }
}

pub fn change_bulk_str(resp: &mut RespVec, data: Vec<u8>) -> bool {
    match resp {
        Resp::Bulk(BulkStr::Str(s)) => {
            *s = data;
            true
        }
        _ => false,
    }
}

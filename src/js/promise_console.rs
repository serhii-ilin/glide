//! Maps live V8 promises to stable console rejection row ids on the JS worker thread.

use std::cell::RefCell;
use std::collections::VecDeque;

const PROMISE_MAP_CAP: usize = 500;

thread_local! {
    static PROMISE_ROWS: RefCell<VecDeque<(v8::Global<v8::Promise>, u64)>> =
        const { RefCell::new(VecDeque::new()) };
    static NEXT_PROMISE_ROW_ID: RefCell<u64> = const { RefCell::new(1) };
}

pub fn clear_promise_rows() {
    PROMISE_ROWS.with(|slot| slot.borrow_mut().clear());
    NEXT_PROMISE_ROW_ID.with(|slot| *slot.borrow_mut() = 1);
}

fn alloc_row_id() -> u64 {
    NEXT_PROMISE_ROW_ID.with(|slot| {
        let mut id = slot.borrow_mut();
        let row = *id;
        *id = id.saturating_add(1);
        row
    })
}

pub fn register_unhandled<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    promise: v8::Local<'s, v8::Promise>,
) -> u64 {
    let row_id = alloc_row_id();
    let global = v8::Global::new(scope, promise);
    PROMISE_ROWS.with(|slot| {
        let mut deque = slot.borrow_mut();
        if deque.len() >= PROMISE_MAP_CAP {
            deque.pop_front();
        }
        deque.push_back((global, row_id));
    });
    row_id
}

pub fn revoke_handled<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    promise: v8::Local<'s, v8::Promise>,
) -> Option<u64> {
    PROMISE_ROWS.with(|slot| {
        let mut deque = slot.borrow_mut();
        let index = deque.iter().position(|(global, _)| {
            let stored = v8::Local::new(scope, global);
            promise.strict_equals(stored.into())
        })?;
        let (_, row_id) = deque.remove(index)?;
        Some(row_id)
    })
}

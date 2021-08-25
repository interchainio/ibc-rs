use async_stream::stream;
use core::mem;
use futures::stream::Stream;

/// ## Example
///
/// ```rust
/// use ibc_relayer::util::stream::try_group_while;
/// use futures::{stream, StreamExt, executor::block_on};
///
/// let input = stream::iter(vec![
///     Ok(1),
///     Ok(1),
///     Ok(2),
///     Err(()),
///     Ok(2),
///     Ok(2),
///     Ok(3),
///     Ok(3),
///     Err(()),
/// ]);
///
/// let output = try_group_while(input, |a, b| a == b).collect::<Vec<_>>();
///
/// assert_eq!(
///     block_on(output),
///     vec![
///         Ok(vec![1, 1]),
///         Ok(vec![2]),
///         Err(()),
///         Ok(vec![2, 2]),
///         Ok(vec![3]),
///         Ok(vec![3]),
///         Err(()),
///     ]
/// );
/// ```
pub fn try_group_while<A, E, S, F>(
    input: S,
    group_these: F,
) -> impl Stream<Item = Result<Vec<A>, E>>
where
    S: Stream<Item = Result<A, E>>,
    F: Fn(&A, &A) -> bool + 'static,
{
    struct State<A> {
        cur: A,
        group: Vec<A>,
    }

    stream! {
        let mut state = None;

        for await x in input {
            match x {
                Ok(x) => {
                    match &mut state {
                        None => {
                            state = Some(State { cur: x, group: vec![] });
                        },
                        Some(state) if group_these(&state.cur, &x) => {
                            let prev = mem::replace(&mut state.cur, x);
                            state.group.push(prev);
                        },
                        Some(state) => {
                            let cur = mem::replace(&mut state.cur, x);
                            state.group.push(cur);
                            let group = mem::take(&mut state.group);
                            yield Ok(group);
                        }
                    }
                }
                Err(e) => {
                    if let Some(cur_state) = mem::take(&mut state) {
                        if !cur_state.group.is_empty() {
                            yield Ok(cur_state.group);
                        }
                        yield Ok(vec![cur_state.cur]);
                    }

                    yield Err(e);
                }
            }
        }

        if let Some(State { cur, mut group }) = state {
            group.push(cur);
            yield Ok(group);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::{executor::block_on, stream, StreamExt};
    use test_env_log::test;

    fn ok<A>(a: A) -> Result<A, ()> {
        Ok(a)
    }

    #[test]
    fn try_group_while_non_empty() {
        let input = stream::iter(vec![
            ok((1, 1)),
            Ok((1, 2)),
            Ok((2, 1)),
            Ok((3, 1)),
            Ok((3, 2)),
            Ok((3, 3)),
            Ok((4, 1)),
            Ok((5, 1)),
            Ok((5, 2)),
        ]);

        let output = try_group_while(input, |a, b| a.0 == b.0).collect::<Vec<_>>();
        let result = block_on(output);

        assert_eq!(
            result,
            vec![
                Ok(vec![(1, 1), (1, 2)]),
                Ok(vec![(2, 1)]),
                Ok(vec![(3, 1), (3, 2), (3, 3)]),
                Ok(vec![(4, 1)]),
                Ok(vec![(5, 1), (5, 2)])
            ]
        );
    }

    #[test]
    fn try_group_while_err() {
        let input = stream::iter(vec![
            ok((1, 1)),
            Ok((1, 2)),
            Ok((2, 1)),
            Ok((3, 1)),
            Ok((3, 2)),
            Err(()),
            Ok((3, 3)),
            Ok((4, 1)),
            Ok((5, 1)),
            Ok((5, 2)),
            Err(()),
        ]);

        let output = try_group_while(input, |a, b| a.0 == b.0).collect::<Vec<_>>();
        let result = block_on(output);

        assert_eq!(
            result,
            vec![
                Ok(vec![(1, 1), (1, 2)]),
                Ok(vec![(2, 1)]),
                Ok(vec![(3, 1)]),
                Ok(vec![(3, 2)]),
                Err(()),
                Ok(vec![(3, 3)]),
                Ok(vec![(4, 1)]),
                Ok(vec![(5, 1)]),
                Ok(vec![(5, 2)]),
                Err(()),
            ]
        );
    }

    #[test]
    fn try_group_while_empty() {
        let input = stream::iter(Vec::<Result<i32, ()>>::new());
        let output = try_group_while(input, |a, b| a == b).collect::<Vec<_>>();
        let result = block_on(output);

        assert_eq!(result, Vec::<Result<Vec<i32>, ()>>::new());
    }
}

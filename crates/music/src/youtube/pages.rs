//! Walking a YouTube listing that comes a page at a time, behind continuation tokens.

use std::future::Future;

use anyhow::Result;

/// The most continuation pages one walk reads, a stop for a token that never ends.
const MAX_PAGES: usize = 100;

/// `items`, the first page of a listing, followed by the pages behind `next`, read through
/// `fetch` until there is no token left, `cap` items are in hand or `MAX_PAGES` pages were read.
/// A page that fails ends the walk with what came before it.
pub(crate) async fn follow<T, F, Fut>(
    mut items: Vec<T>,
    mut next: Option<String>,
    cap: usize,
    mut fetch: F,
) -> Vec<T>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<(Vec<T>, Option<String>)>>,
{
    for _ in 0..MAX_PAGES {
        if items.len() >= cap {
            break;
        }
        let Some(token) = next.take() else {
            break;
        };
        match fetch(token).await {
            Ok((more, after)) => {
                items.extend(more);
                next = after;
            }
            Err(error) => {
                log::warn!("youtube: cannot read the next page, keeping what came: {error:#}");
                break;
            }
        }
    }
    items.truncate(cap);
    items
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use anyhow::anyhow;

    use super::*;

    /// A fetcher over numbered pages: the token `"n"` answers page `n` and leads to `n + 1`,
    /// and the token `"last"` leads nowhere. Every token asked for is written down.
    async fn walk(
        first: Vec<u32>,
        next: Option<&str>,
        cap: usize,
        failing: Option<&str>,
        asked: &RefCell<Vec<String>>,
    ) -> Vec<u32> {
        follow(first, next.map(str::to_owned), cap, |token| {
            asked.borrow_mut().push(token.clone());
            let answer = match (token.as_str(), failing) {
                (token, Some(failing)) if token == failing => Err(anyhow!("gone")),
                ("last", _) => Ok((vec![99], None)),
                (number, _) => {
                    let page: u32 = number.parse().unwrap();
                    Ok((vec![page * 10, page * 10 + 1], Some((page + 1).to_string())))
                }
            };
            async move { answer }
        })
        .await
    }

    #[tokio::test]
    async fn follows_every_token_until_there_is_none() {
        let asked = RefCell::new(Vec::new());

        let items = walk(vec![1], Some("last"), 500, None, &asked).await;

        assert_eq!(items, [1, 99]);
        assert_eq!(*asked.borrow(), ["last"]);
    }

    #[tokio::test]
    async fn a_first_page_without_a_token_is_all_there_is() {
        let asked = RefCell::new(Vec::new());

        assert_eq!(walk(vec![1, 2], None, 500, None, &asked).await, [1, 2]);
        assert!(asked.borrow().is_empty());
    }

    #[tokio::test]
    async fn stops_at_the_cap() {
        let asked = RefCell::new(Vec::new());

        let items = walk(vec![1], Some("1"), 4, None, &asked).await;

        assert_eq!(items, [1, 10, 11, 20]);
        assert_eq!(*asked.borrow(), ["1", "2"]);
    }

    #[tokio::test]
    async fn stops_after_the_most_pages_for_a_token_that_never_ends() {
        let asked = RefCell::new(Vec::new());

        let items = walk(Vec::new(), Some("1"), usize::MAX, None, &asked).await;

        assert_eq!(asked.borrow().len(), MAX_PAGES);
        assert_eq!(items.len(), MAX_PAGES * 2);
    }

    #[tokio::test]
    async fn a_failing_page_keeps_what_came_before_it() {
        let asked = RefCell::new(Vec::new());

        let items = walk(vec![1], Some("1"), 500, Some("2"), &asked).await;

        assert_eq!(items, [1, 10, 11]);
    }
}

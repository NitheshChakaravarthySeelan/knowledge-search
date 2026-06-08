/// Processes a large slice of items in smaller batches using a provided async function.
pub async fn process_in_batches<T, F, Fut, R>(
    items: &[T],
    batch_size: usize,
    mut f: F,
) -> anyhow::Result<Vec<R>>
where
    F: FnMut(&[T]) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Vec<R>>>,
{
    let mut all_results = Vec::with_capacity(items.len());

    for batch in items.chunks(batch_size) {
        let results = f(batch).await?;
        all_results.extend(results);
    }

    Ok(all_results)
}

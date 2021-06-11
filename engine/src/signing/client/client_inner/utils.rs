#[allow(dead_code)]
pub fn reorg_vector<T: Clone>(v: &mut Vec<T>, order: &[usize]) {
    assert_eq!(v.len(), order.len());

    let owned_v = v.split_off(0);

    let mut combined: Vec<_> = owned_v.into_iter().zip(order.iter()).collect();

    combined.sort_by_key(|(_data, idx)| *idx);

    *v = combined.into_iter().map(|(data, _idx)| data).collect();
}

#[cfg(test)]
#[test]
fn reorg_vector_works() {
    {
        let mut v = vec![1, 2, 3];
        let order = [2, 1, 3];
        reorg_vector(&mut v, &order);
        assert_eq!(v, [2, 1, 3]);
    }

    {
        let mut v = vec![2, 1, 3];
        let order = [3, 2, 1];
        reorg_vector(&mut v, &order);
        assert_eq!(v, [3, 1, 2]);
    }
}

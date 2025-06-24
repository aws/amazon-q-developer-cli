use std::hash::Hash;

use dashmap::DashMap;
use fig_proto::fig::NotificationType;
use fnv::FnvBuildHasher;

pub struct NotificationHandler<K>
where
    K: Hash + Eq + PartialEq,
{
    pub subscriptions: DashMap<(K, NotificationType), i64, FnvBuildHasher>,
}

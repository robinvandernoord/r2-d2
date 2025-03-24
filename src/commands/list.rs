use crate::r2::{Direction, QueryString, to_query_part};

#[derive(Debug, Default)]
pub struct ListOptions {
    cursor: Option<String>,
    direction: Option<Direction>,
    order: Option<String>,
    per_page: Option<u32>,
    start_after: Option<String>,
}

impl QueryString for ListOptions {
    fn to_query(&self) -> String {
        let mut parts = vec![];

        if let Some(cursor) = &self.cursor {
            parts.push(to_query_part("cursor", cursor));
        }

        if let Some(direction) = &self.direction {
            parts.push(to_query_part("direction", direction.to_string()));
        }

        if let Some(order) = &self.order {
            parts.push(to_query_part("order", order));
        }

        if let Some(per_page) = &self.per_page {
            parts.push(to_query_part("per_page", per_page.to_string()));
        }

        if let Some(start_after) = &self.start_after {
            parts.push(to_query_part("start_after", start_after));
        }

        parts.join("&")
    }
}

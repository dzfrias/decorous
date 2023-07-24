use crate::location::Location;

#[derive(Debug, Clone)]
pub struct FragmentMetadata {
    id: u32,
    parent_id: Option<u32>,
    location: Location,
    scope: Option<u32>,
}

impl FragmentMetadata {
    pub fn new(id: u32, parent_id: Option<u32>, location: Location, scope: Option<u32>) -> Self {
        Self {
            id,
            parent_id,
            location,
            scope,
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn parent_id(&self) -> Option<u32> {
        self.parent_id
    }

    pub fn set_parent_id(&mut self, parent_id: Option<u32>) {
        self.parent_id = parent_id;
    }

    pub fn location(&self) -> Location {
        self.location
    }

    pub fn scope(&self) -> Option<u32> {
        self.scope
    }
}

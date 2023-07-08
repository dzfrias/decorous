use crate::ast::Location;

#[derive(Debug)]
pub struct FragmentMetadata {
    id: u32,
    parent_id: Option<u32>,
    location: Location,
}

impl FragmentMetadata {
    pub fn new(id: u32, parent_id: Option<u32>, location: Location) -> Self {
        Self {
            id,
            parent_id,
            location,
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
}

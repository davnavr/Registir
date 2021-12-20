use crate::assembler::Error;

#[derive(Debug)]
pub struct Once<'a, N, T, E> {
    value: Option<T>,
    error: E,
    phantom: std::marker::PhantomData<&'a N>,
}

impl<'a, N, T, E: Fn(N, &T) -> Error> Once<'a, N, T, E> {
    pub fn new(error: E) -> Self {
        Self {
            value: None,
            error,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn is_set(&self) -> bool {
        self.value.is_some()
    }

    pub fn value(self) -> Option<T> {
        self.value
    }

    pub fn declare(&mut self, errors: &mut Vec<Error>, node: N) -> Option<impl FnOnce(T) + '_> {
        match self.value {
            None => Some(|value: T| {
                self.value.replace(value);
            }),
            Some(ref existing) => {
                errors.push((self.error)(node, existing));
                None
            }
        }
    }

    pub fn declare_and_set(&mut self, errors: &mut Vec<Error>, node: N, value: T) {
        if let Some(setter) = self.declare(errors, node) {
            setter(value)
        }
    }
}

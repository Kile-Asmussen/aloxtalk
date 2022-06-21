trait Monadics: Sized {
    fn tap<F>(&self, f: F) -> &Self
    where
        F: FnOnce(&Self),
    {
        f(self);
        self
    }

    fn with<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut Self),
    {
        f(self);
        self
    }

    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }

    fn view<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Self) -> R,
    {
        f(self)
    }

    fn then<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        f(self)
    }
}

impl<T> Monadics for T {}

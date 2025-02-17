/*!
   Tagged data types with a single type tag.

   This is mainly used to tag data types that are associated
   to a single chain and do not uniquely correspond to any
   resource on a counterparty chain.

    Example:

    - [`Tagged<Chain, ChainId>`](crate::types::id::TaggedChainId) -
      A chain ID belongs to a chain and do not uniquely
      correspond to a counterparty chain.

    - [`Tagged<Chain, Wallet>`](crate::types::wallet::Wallet) -
      A wallet belongs to a chain and do not uniquely
      correspond to a counterparty chain

*/

use core::cmp::Ordering;
use core::fmt::{self, Debug, Display};
use core::marker::PhantomData;
use serde::{Serialize, Serializer};

use super::dual::Tagged as DualTagged;

/**
   Tag a `Value` type with a single `Tag` type tag.
*/
pub struct Tagged<Tag, Value>(pub Value, pub PhantomData<Tag>);

impl<Tag, Value> Tagged<Tag, Value> {
    /**
       Create a new tagged value with any type tag.

       Example:

       ```rust
       # use ibc_test_framework::types::tagged::mono::Tagged;
       struct Foo;

       let val: Tagged<Foo, i64> = Tagged::new(42);
       ```
    */
    pub fn new(value: Value) -> Self {
        Tagged(value, PhantomData)
    }

    /**
        Get a reference to the underlying value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;

        let val1: Tagged<Foo, i64> = Tagged::new(42);
        let val2: &i64 = val1.value();
        ```
    */
    pub fn value(&self) -> &Value {
        &self.0
    }

    /**
        Get a mutable reference to the underlying value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;

        let mut val1: Tagged<Foo, i64> = Tagged::new(42);
        let val2: &mut i64 = val1.mut_value();
        ```
    */
    pub fn mut_value(&mut self) -> &mut Value {
        &mut self.0
    }

    /**
        Convert the tagged value into an untagged value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;

        let val1: Tagged<Foo, i64> = Tagged::new(42);
        let val2: i64 = val1.into_value();
        ```
    */
    pub fn into_value(self) -> Value {
        self.0
    }

    /**
        Convert a tagged value into a tagged reference.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;

        let val1: Tagged<Foo, i64> = Tagged::new(42);
        let val2: Tagged<Foo, &i64> = val1.as_ref();
        ```
    */
    pub fn as_ref(&self) -> Tagged<Tag, &Value> {
        Tagged::new(&self.0)
    }

    /**
        Retag a tagged value with a different tag.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;
        struct Bar;

        let val1: Tagged<Foo, i64> = Tagged::new(42);
        let val2: Tagged<Bar, i64> = val1.retag();
        ```
    */
    pub fn retag<TagB>(self) -> Tagged<TagB, Value> {
        Tagged::new(self.0)
    }

    /**
        Add an additional tag to a mono-tagged value, turning
        it into a [`DualTagged`] value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        # use ibc_test_framework::types::tagged::dual::Tagged as DualTagged;
        struct Foo;
        struct Bar;

        let val1: Tagged<Foo, i64> = Tagged::new(42);
        let val2: DualTagged<Foo, Bar, i64> = val1.add_tag();
        ```
    */
    pub fn add_tag<TagB>(self) -> DualTagged<Tag, TagB, Value> {
        DualTagged::new(self.into_value())
    }

    /**
        Perform operation with the reference to the underlying reference,
        and have result that preserve the tag.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;

        let val1: Tagged<Foo, i64> = Tagged::new(42);
        let val2: Tagged<Foo, String> = val1.map(|x| format!("{}", x));
        ```
    */
    pub fn map<T>(&self, mapper: impl FnOnce(&Value) -> T) -> Tagged<Tag, T> {
        Tagged::new(mapper(self.value()))
    }

    /**
        Perform operation with the reference to the underlying reference,
        and have result reference with the same lifetime that preserve the tag.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Person { name: String, age: u8 }
        struct Alice;

        let person: Tagged<Alice, Person> = Tagged::new(Person {
            name: "Alice".to_string(),
            age: 30,
        });

        let name: Tagged<Alice, &str> = person.map_ref(|person| person.name.as_str());
        ```
    */
    pub fn map_ref<'a, T: ?Sized>(
        &'a self,
        mapper: impl FnOnce(&'a Value) -> &'a T,
    ) -> Tagged<Tag, &'a T> {
        Tagged::new(mapper(self.value()))
    }

    /**
        Perform an operation consuming the original tagged value, and return
        a result value preserving the original tag.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Person { name: String, age: u8 }
        struct Alice;

        let person: Tagged<Alice, Person> = Tagged::new(Person {
            name: "Alice".to_string(),
            age: 30,
        });

        let name: Tagged<Alice, String> = person.map_into(|person| person.name);
        ```
    */
    pub fn map_into<T>(self, mapper: impl FnOnce(Value) -> T) -> Tagged<Tag, T> {
        Tagged::new(mapper(self.0))
    }
}

impl<Tag, Value: Clone> Tagged<Tag, &Value> {
    /**
        Convert a [`Clone`]eable tagged reference into a tagged value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;

        let val1: String = "foo".to_string();
        let val2: Tagged<Foo, &String> = Tagged::new(&val1);
        let val3: Tagged<Foo, String> = val2.cloned();
        ```
    */
    pub fn cloned(&self) -> Tagged<Tag, Value> {
        Tagged::new(self.0.clone())
    }

    pub fn cloned_value(&self) -> Value {
        self.0.clone()
    }
}

impl<Tag, Value> Tagged<Tag, Option<Value>> {
    /**
        Convert a tagged [`Option`] value into an optional tagged value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;

        let val1: Tagged<Foo, Option<i64>> = Tagged::new(Some(8));
        let val2: Option<Tagged<Foo, i64>> = val1.transpose();
        ```
    */
    pub fn transpose(self) -> Option<Tagged<Tag, Value>> {
        self.0.map(Tagged::new)
    }
}

impl<Tag, Value, E> Tagged<Tag, Result<Value, E>> {
    /**
        Convert a tagged [`Result`] value into a result tagged value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;
        struct Error;

        let val1: Tagged<Foo, Result<i64, Error>> = Tagged::new(Ok(8));
        let val2: Result<Tagged<Foo, i64>, Error> = val1.transpose();
        ```
    */
    pub fn transpose(self) -> Result<Tagged<Tag, Value>, E> {
        self.0.map(Tagged::new)
    }
}

impl<Tag, Value> Tagged<Tag, Vec<Value>> {
    /**
        Convert a tagged [`Vec`] value into a list of tagged value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::mono::Tagged;
        struct Foo;

        let val1: Tagged<Foo, Vec<i64>> = Tagged::new(vec![1, 2, 3]);
        let val2: Vec<Tagged<Foo, i64>> = val1.transpose();
        ```
    */
    pub fn transpose(self) -> Vec<Tagged<Tag, Value>> {
        self.into_iter().collect()
    }
}

impl<Tag, Value> AsRef<Value> for Tagged<Tag, &Value> {
    fn as_ref(&self) -> &Value {
        self.value()
    }
}

impl<Tag, Value> AsRef<Value> for Tagged<Tag, Value> {
    fn as_ref(&self) -> &Value {
        self.value()
    }
}

impl<Tag, Value: Serialize> Serialize for Tagged<Tag, Value> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

/**
   Create a tagged iterator, if the underlying value supports iteration.

   Example:

   ```rust
   # use ibc_test_framework::types::tagged::mono::Tagged;
   struct Foo;

   let values: Tagged<Foo, Vec<i64>> = Tagged::new(vec![1, 2, 3]);
   for value in values.into_iter() {
       let value: Tagged<Foo, i64> = value;
       // do something
   }
   ```
*/
pub struct TaggedIterator<Tag, It>(Tagged<Tag, It>);

impl<Tag, It: Iterator> Iterator for TaggedIterator<Tag, It> {
    type Item = Tagged<Tag, It::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0 .0.next().map(Tagged::new)
    }
}

impl<Tag, Value: IntoIterator> IntoIterator for Tagged<Tag, Value> {
    type Item = Tagged<Tag, Value::Item>;

    type IntoIter = TaggedIterator<Tag, Value::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        TaggedIterator(self.map_into(|v| v.into_iter()))
    }
}

impl<Tag, Value: Copy> Copy for Tagged<Tag, Value> {}

unsafe impl<Tag, Value: Send> Send for Tagged<Tag, Value> {}
unsafe impl<Tag, Value: Sync> Sync for Tagged<Tag, Value> {}

impl<Tag, Value: Clone> Clone for Tagged<Tag, Value> {
    fn clone(&self) -> Self {
        Self::new(self.0.clone())
    }
}

impl<Tag, Value: Default> Default for Tagged<Tag, Value> {
    fn default() -> Self {
        Self::new(Value::default())
    }
}

impl<Tag, Value: Debug> Debug for Tagged<Tag, Value> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(self.value(), f)
    }
}

impl<Tag, Value: Display> Display for Tagged<Tag, Value> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self.value(), f)
    }
}

impl<Tag, Value: PartialEq> PartialEq for Tagged<Tag, Value> {
    fn eq(&self, other: &Self) -> bool {
        self.value().eq(other.value())
    }
}

impl<Tag, Value: Eq> Eq for Tagged<Tag, Value> {}

impl<Tag, Value: PartialOrd> PartialOrd for Tagged<Tag, Value> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value().partial_cmp(other.value())
    }
}

impl<Tag, Value: Ord> Ord for Tagged<Tag, Value> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value().cmp(other.value())
    }
}

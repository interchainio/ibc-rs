/*!
   Tagged data types with two type tags.

   This is mainly used to tag data types that are associated
   to a single chain and also uniquely correspond to some
   resource on a counterparty chain.

    Example:

    - [`Tagged<ChainA, ChainB, ChannelId>`](crate::types::id::TaggedChannelId) -
      A channel ID belongs to a chain `ChainA`, and it is also uniquely
      corresponds to a channel connected to a counterparty chain `ChainB`.
*/

use core::cmp::Ordering;
use core::fmt::{self, Debug, Display};
use core::marker::PhantomData;

/**
   Tag a `Value` type with a two type tags `TagA` and `TagB`.
*/
pub struct Tagged<TagA, TagB, Value>(pub Value, PhantomData<(TagA, TagB)>);

impl<TagA, TagB, Value> Tagged<TagA, TagB, Value> {
    /**
       Create a new tagged value with any type tag.

       Example:

       ```rust
       # use ibc_test_framework::types::tagged::dual::Tagged;
       struct Foo;
       struct Bar;

       let val: Tagged<Foo, Bar, i64> = Tagged::new(42);
       ```
    */
    pub fn new(value: Value) -> Self {
        Tagged(value, PhantomData)
    }

    /**
        Get a reference to the underlying value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;

        let val1: Tagged<Foo, Bar, i64> = Tagged::new(42);
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
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;

        let mut val1: Tagged<Foo, Bar, i64> = Tagged::new(42);
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
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;

        let val1: Tagged<Foo, Bar, i64> = Tagged::new(42);
        let val2: Tagged<Foo, Bar, &i64> = val1.as_ref();
        ```
    */
    pub fn as_ref(&self) -> Tagged<TagA, TagB, &Value> {
        Tagged::new(&self.0)
    }

    /**
        Flips the ordering of the two tags.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;

        let val1: Tagged<Foo, Bar, i64> = Tagged::new(42);
        let val2: Tagged<Bar, Foo, i64> = val1.flip();
        ```
    */
    pub fn flip(self) -> Tagged<TagB, TagA, Value> {
        Tagged::new(self.0)
    }

    /**
        Retag a tagged value with a different tag.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;
        struct Baz;
        struct Quux;

        let val1: Tagged<Foo, Bar, i64> = Tagged::new(42);
        let val2: Tagged<Baz, Quux, i64> = val1.retag();
        ```
    */
    pub fn retag<TagC, TagD>(self) -> Tagged<TagC, TagD, Value> {
        Tagged::new(self.0)
    }

    /**
        Perform operation with the reference to the underlying reference,
        and have result that preserve the tag.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;

        let val1: Tagged<Foo, Bar, i64> = Tagged::new(42);
        let val2: Tagged<Foo, Bar, String> = val1.map(|x| format!("{}", x));
        ```
    */
    pub fn map<T>(&self, mapper: impl FnOnce(&Value) -> T) -> Tagged<TagA, TagB, T> {
        Tagged::new(mapper(&self.0))
    }

    /**
        Perform operation with the reference to the underlying reference,
        and have result reference with the same lifetime that preserve the tags.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Person { name: String, age: u8 }
        struct Alice;
        struct Wonderland;

        let person: Tagged<Alice, Wonderland, Person> = Tagged::new(Person {
            name: "Alice".to_string(),
            age: 30,
        });

        let name: Tagged<Alice, Wonderland, &str> = person
            .map_ref(|person| person.name.as_str());
        ```
    */
    pub fn map_ref<'a, T: ?Sized>(
        &'a self,
        mapper: impl FnOnce(&'a Value) -> &'a T,
    ) -> Tagged<TagA, TagB, &'a T> {
        Tagged::new(mapper(self.value()))
    }

    /**
        Perform an operation consuming the original tagged value, and return
        a result value preserving the original tag.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Person { name: String, age: u8 }
        struct Alice;
        struct Wonderland;

        let person: Tagged<Alice, Wonderland, Person> = Tagged::new(Person {
            name: "Alice".to_string(),
            age: 30,
        });

        let name: Tagged<Alice, Wonderland, String> = person.map_into(|person| person.name);
        ```
    */
    pub fn map_into<T>(self, mapper: impl FnOnce(Value) -> T) -> Tagged<TagA, TagB, T> {
        Tagged::new(mapper(self.0))
    }

    /**
        Perform operation with the reference to the underlying reference,
        and have two tags flipped in the result.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;

        let val1: Tagged<Foo, Bar, i64> = Tagged::new(42);
        let val2: Tagged<Bar, Foo, String> = val1.contra_map(|x| format!("{}", x));
        ```

        This is mainly useful for accessing IBC data structures that may contain
        information about the counterparty chain. For example, consider
        a tagged and simplified version of
        [`ConnectionEnd`](ibc_relayer_types::core::ics03_connection::connection::ConnectionEnd):

        ```rust
        # use ibc_relayer_types::core::ics24_host::identifier::ConnectionId;
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct ConnectionEnd {
            connection_id: ConnectionId,
            counterparty_connection_id: ConnectionId,
        }

        fn process_connection_end<ChainA, ChainB>(
            connection_end: Tagged<ChainA, ChainB, ConnectionEnd>)
        {
            let connection_id: Tagged<ChainA, ChainB, ConnectionId> =
                connection_end.map(|c| c.connection_id.clone());

            let counterparty_connection_id: Tagged<ChainB, ChainA, ConnectionId> =
                connection_end.contra_map(|c| c.connection_id.clone());

            // do something
        }
        ```

        The `ConnectionEnd` data type above is a _bidirectional_ data type that
        contains fields that are specific to both chains: the connection ID
        and the counterparty connection ID. But when we tag the `ConnectionEnd`
        type, we have to choose one dominant chain to appear at the first position.

        When we extract the `connection_id` field, we use `map` to preserve the
        tag ordering to say that the connection ID _belongs_ to the `ChainA`,
        and corresponds to a connection to the counterparty `ChainB`.

        When we extract the `counterparty_connection_id` field, we use
        `contra_map` to flip the tag ordering to say that the connection ID
        _belongs_ to the counterparty `ChainB`, and corresponds to a connection
        to `ChainA`.
    */
    pub fn contra_map<T>(&self, mapper: impl FnOnce(&Value) -> T) -> Tagged<TagB, TagA, T> {
        Tagged::new(mapper(&self.0))
    }

    /**
       Perform operation with the reference to the underlying reference,
       and have the result reference with the same lifetime and have the
       two tags flipped in the result.

       Example:

       ```rust
       # use ibc_test_framework::types::tagged::dual::Tagged;
       struct Person { name: String, age: u8 }
       struct Alice;
       struct Wonderland;

       let person: Tagged<Alice, Wonderland, Person> = Tagged::new(Person {
           name: "Alice".to_string(),
           age: 30,
       });

       let name: Tagged<Wonderland, Alice, &str> = person
           .contra_map_ref(|person| person.name.as_str());
       ```
    */
    pub fn contra_map_ref<'a, T: ?Sized>(
        &'a self,
        mapper: impl FnOnce(&'a Value) -> &'a T,
    ) -> Tagged<TagB, TagA, &'a T> {
        Tagged::new(mapper(self.value()))
    }

    /**
        Perform operation consuming the underlying reference,
        and have two tags switched in the result.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;

        let val1: Tagged<Foo, Bar, i64> = Tagged::new(42);
        let val2: Tagged<Bar, Foo, String> = val1.contra_map_into(|x| format!("{}", x));
        ```
    */
    pub fn contra_map_into<T>(self, mapper: impl FnOnce(Value) -> T) -> Tagged<TagB, TagA, T> {
        Tagged::new(mapper(self.0))
    }
}

impl<TagA, TagB, Value: Clone> Tagged<TagA, TagB, &Value> {
    /**
        Convert a [`Clone`]eable tagged reference into a tagged value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;

        let val1: String = "foo".to_string();
        let val2: Tagged<Foo, Bar, &String> = Tagged::new(&val1);
        let val3: Tagged<Foo, Bar, String> = val2.cloned();
        ```
    */
    pub fn cloned(&self) -> Tagged<TagA, TagB, Value> {
        Tagged::new(self.0.clone())
    }

    pub fn cloned_value(&self) -> Value {
        self.0.clone()
    }
}

impl<TagA, TagB, Value> Tagged<TagA, TagB, Option<Value>> {
    /**
        Convert a tagged [`Option`] value into an optional tagged value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;

        let val1: Tagged<Foo, Bar, Option<i64>> = Tagged::new(Some(8));
        let val2: Option<Tagged<Foo, Bar, i64>> = val1.transpose();
        ```
    */
    pub fn transpose(self) -> Option<Tagged<TagA, TagB, Value>> {
        self.0.map(Tagged::new)
    }
}

impl<TagA, TagB, Value, E> Tagged<TagA, TagB, Result<Value, E>> {
    /**
        Convert a tagged [`Result`] value into a result tagged value.

        Example:

        ```rust
        # use ibc_test_framework::types::tagged::dual::Tagged;
        struct Foo;
        struct Bar;
        struct Error;

        let val1: Tagged<Foo, Bar, Result<i64, Error>> = Tagged::new(Ok(8));
        let val2: Result<Tagged<Foo, Bar, i64>, Error> = val1.transpose();
        ```
    */
    pub fn transpose(self) -> Result<Tagged<TagA, TagB, Value>, E> {
        self.0.map(Tagged::new)
    }
}

impl<TagA, TagB, Value> AsRef<Value> for Tagged<TagA, TagB, &Value> {
    fn as_ref(&self) -> &Value {
        self.value()
    }
}

impl<TagA, TagB, Value> AsRef<Value> for Tagged<TagA, TagB, Value> {
    fn as_ref(&self) -> &Value {
        self.value()
    }
}

impl<TagA, TagB, Value: Copy> Copy for Tagged<TagA, TagB, Value> {}

unsafe impl<TagA, TagB, Value: Send> Send for Tagged<TagA, TagB, Value> {}
unsafe impl<TagA, TagB, Value: Sync> Sync for Tagged<TagA, TagB, Value> {}

impl<TagA, TagB, Value: Clone> Clone for Tagged<TagA, TagB, Value> {
    fn clone(&self) -> Self {
        Self::new(self.0.clone())
    }
}

impl<TagA, TagB, Value: Debug> Debug for Tagged<TagA, TagB, Value> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(self.value(), f)
    }
}

impl<TagA, TagB, Value: Display> Display for Tagged<TagA, TagB, Value> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self.value(), f)
    }
}

impl<TagA, TagB, Value: PartialEq> PartialEq for Tagged<TagA, TagB, Value> {
    fn eq(&self, other: &Self) -> bool {
        self.value().eq(other.value())
    }
}

impl<TagA, TagB, Value: Eq> Eq for Tagged<TagA, TagB, Value> {}

impl<TagA, TagB, Value: PartialOrd> PartialOrd for Tagged<TagA, TagB, Value> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value().partial_cmp(other.value())
    }
}

impl<TagA, TagB, Value: Ord> Ord for Tagged<TagA, TagB, Value> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value().cmp(other.value())
    }
}

/**
   Create a tagged iterator, if the underlying value supports iteration.

   Example:

   ```rust
   # use ibc_test_framework::types::tagged::dual::Tagged;
   struct Foo;
   struct Bar;

   let values: Tagged<Foo, Bar, Vec<i64>> = Tagged::new(vec![1, 2, 3]);
   for value in values.into_iter() {
       let value: Tagged<Foo, Bar, i64> = value;
       // do something
   }
   ```
*/
pub struct TaggedIterator<TagA, TagB, It>(Tagged<TagA, TagB, It>);

impl<TagA, TagB, It: Iterator> Iterator for TaggedIterator<TagA, TagB, It> {
    type Item = Tagged<TagA, TagB, It::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0 .0.next().map(Tagged::new)
    }
}

impl<TagA, TagB, Value: IntoIterator> IntoIterator for Tagged<TagA, TagB, Value> {
    type Item = Tagged<TagA, TagB, Value::Item>;

    type IntoIter = TaggedIterator<TagA, TagB, Value::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        TaggedIterator(self.map_into(|v| v.into_iter()))
    }
}

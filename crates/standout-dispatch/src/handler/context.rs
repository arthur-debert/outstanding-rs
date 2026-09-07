use standout_types::{ColorPolicy, Representation};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
#[derive(Default)]
pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any>>,
}
impl Extensions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert<T: 'static>(&mut self, val: T) -> Option<T> {
        self.map
            .insert(TypeId::of::<T>(), Box::new(val))
            .and_then(|boxed| boxed.downcast().ok().map(|b| *b))
    }
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref())
    }
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_mut())
    }
    pub fn get_required<T: 'static>(&self) -> Result<&T, anyhow::Error> {
        self.get::<T>().ok_or_else(|| {
            anyhow::anyhow!(
                "Extension missing: type {} not found in context",
                std::any::type_name::<T>()
            )
        })
    }
    pub fn get_mut_required<T: 'static>(&mut self) -> Result<&mut T, anyhow::Error> {
        self.get_mut::<T>().ok_or_else(|| {
            anyhow::anyhow!(
                "Extension missing: type {} not found in context",
                std::any::type_name::<T>()
            )
        })
    }
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast().ok().map(|b| *b))
    }
    pub fn contains<T: 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn clear(&mut self) {
        self.map.clear();
    }
}
impl fmt::Debug for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extensions")
            .field("len", &self.map.len())
            .finish_non_exhaustive()
    }
}
impl Clone for Extensions {
    // `Box<dyn Any>` isn't `Clone`: a clone starts empty.
    fn clone(&self) -> Self {
        Self::new()
    }
}
#[derive(Debug)]
pub struct CommandContext {
    pub command_path: Vec<String>,
    pub app_state: Rc<Extensions>,
    pub extensions: Extensions,
    representation: Representation,
    color_policy: ColorPolicy,
}
impl CommandContext {
    pub fn new(command_path: Vec<String>, app_state: Rc<Extensions>) -> Self {
        Self {
            command_path,
            app_state,
            extensions: Extensions::new(),
            representation: Representation::default(),
            color_policy: ColorPolicy::default(),
        }
    }
    pub fn with_presentation(
        mut self,
        representation: Representation,
        color_policy: ColorPolicy,
    ) -> Self {
        self.representation = representation;
        self.color_policy = color_policy;
        self
    }
    pub fn representation(&self) -> Representation {
        self.representation
    }
    pub fn color_policy(&self) -> ColorPolicy {
        self.color_policy
    }
}
impl Default for CommandContext {
    fn default() -> Self {
        Self::new(Vec::new(), Rc::new(Extensions::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_command_context_creation() {
        let ctx = CommandContext::new(
            vec!["config".into(), "get".into()],
            Rc::new(Extensions::new()),
        );
        assert_eq!(ctx.command_path, vec!["config", "get"]);
    }
    #[test]
    fn test_command_context_default() {
        let ctx = CommandContext::default();
        assert!(ctx.command_path.is_empty());
        assert!(ctx.extensions.is_empty());
        assert!(ctx.app_state.is_empty());
    }
    #[test]
    fn test_command_context_with_app_state() {
        struct Database {
            url: String,
        }
        struct Config {
            debug: bool,
        }
        let mut app_state = Extensions::new();
        app_state.insert(Database {
            url: "postgres://localhost".into(),
        });
        app_state.insert(Config { debug: true });
        let app_state = Rc::new(app_state);
        let ctx = CommandContext::new(vec!["list".into()], app_state.clone());
        let db = ctx.app_state.get::<Database>().unwrap();
        assert_eq!(db.url, "postgres://localhost");
        let config = ctx.app_state.get::<Config>().unwrap();
        assert!(config.debug);
        assert_eq!(Rc::strong_count(&ctx.app_state), 2);
    }
    #[test]
    fn test_command_context_app_state_get_required() {
        struct Present;
        let mut app_state = Extensions::new();
        app_state.insert(Present);
        let ctx = CommandContext::new(vec![], Rc::new(app_state));
        assert!(ctx.app_state.get_required::<Present>().is_ok());
        #[derive(Debug)]
        struct Missing;
        let err = ctx.app_state.get_required::<Missing>();
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Extension missing"));
    }
    #[test]
    fn test_extensions_insert_and_get() {
        struct MyState {
            value: i32,
        }
        let mut ext = Extensions::new();
        assert!(ext.is_empty());
        ext.insert(MyState { value: 42 });
        assert!(!ext.is_empty());
        assert_eq!(ext.len(), 1);
        let state = ext.get::<MyState>().unwrap();
        assert_eq!(state.value, 42);
    }
    #[test]
    fn test_extensions_get_mut() {
        struct Counter {
            count: i32,
        }
        let mut ext = Extensions::new();
        ext.insert(Counter { count: 0 });
        if let Some(counter) = ext.get_mut::<Counter>() {
            counter.count += 1;
        }
        assert_eq!(ext.get::<Counter>().unwrap().count, 1);
    }
    #[test]
    fn test_extensions_multiple_types() {
        struct TypeA(i32);
        struct TypeB(String);
        let mut ext = Extensions::new();
        ext.insert(TypeA(1));
        ext.insert(TypeB("hello".into()));
        assert_eq!(ext.len(), 2);
        assert_eq!(ext.get::<TypeA>().unwrap().0, 1);
        assert_eq!(ext.get::<TypeB>().unwrap().0, "hello");
    }
    #[test]
    fn test_extensions_replace() {
        struct Value(i32);
        let mut ext = Extensions::new();
        ext.insert(Value(1));
        let old = ext.insert(Value(2));
        assert_eq!(old.unwrap().0, 1);
        assert_eq!(ext.get::<Value>().unwrap().0, 2);
    }
    #[test]
    fn test_extensions_remove() {
        struct Value(i32);
        let mut ext = Extensions::new();
        ext.insert(Value(42));
        let removed = ext.remove::<Value>();
        assert_eq!(removed.unwrap().0, 42);
        assert!(ext.is_empty());
        assert!(ext.get::<Value>().is_none());
    }
    #[test]
    fn test_extensions_contains() {
        struct Present;
        struct Absent;
        let mut ext = Extensions::new();
        ext.insert(Present);
        assert!(ext.contains::<Present>());
        assert!(!ext.contains::<Absent>());
    }
    #[test]
    fn test_extensions_clear() {
        struct A;
        struct B;
        let mut ext = Extensions::new();
        ext.insert(A);
        ext.insert(B);
        assert_eq!(ext.len(), 2);
        ext.clear();
        assert!(ext.is_empty());
    }
    #[test]
    fn test_extensions_missing_type_returns_none() {
        struct NotInserted;
        let ext = Extensions::new();
        assert!(ext.get::<NotInserted>().is_none());
    }
    #[test]
    fn test_extensions_get_required() {
        #[derive(Debug)]
        struct Config {
            value: i32,
        }
        let mut ext = Extensions::new();
        ext.insert(Config { value: 100 });
        let val = ext.get_required::<Config>();
        assert!(val.is_ok());
        assert_eq!(val.unwrap().value, 100);
        #[derive(Debug)]
        struct Missing;
        let err = ext.get_required::<Missing>();
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Extension missing: type"));
    }
    #[test]
    fn test_extensions_get_mut_required() {
        #[derive(Debug)]
        struct State {
            count: i32,
        }
        let mut ext = Extensions::new();
        ext.insert(State { count: 0 });
        {
            let val = ext.get_mut_required::<State>();
            assert!(val.is_ok());
            val.unwrap().count += 1;
        }
        assert_eq!(ext.get_required::<State>().unwrap().count, 1);
        #[derive(Debug)]
        struct Missing;
        let err = ext.get_mut_required::<Missing>();
        assert!(err.is_err());
    }
    #[test]
    fn test_extensions_clone_behavior() {
        struct Data(#[allow(dead_code)] i32);
        let mut original = Extensions::new();
        original.insert(Data(42));
        let cloned = original.clone();
        assert!(original.get::<Data>().is_some());
        assert!(cloned.is_empty());
        assert!(cloned.get::<Data>().is_none());
    }
}

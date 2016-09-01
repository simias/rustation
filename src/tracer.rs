//! Interface used to log internal variables in order to generate
//! traces

use std::collections::HashMap;

pub type ValueType  = u32;
pub type ValueSize  = u8;

/// Underlying type of every logged value. Since we use a `u32` we
/// only support variables up to 32bits for now. The 2nd parameter is
/// the size of the value in bits.
#[derive(Copy, Clone)]
pub struct SizedValue(pub ValueType, pub ValueSize);

impl From<bool> for SizedValue {
    fn from(v: bool) -> SizedValue {
        SizedValue(v as ValueType, 1)
    }
}

impl From<u8> for SizedValue {
    fn from(v: u8) -> SizedValue {
        SizedValue(v as ValueType, 8)
    }
}

impl From<u16> for SizedValue {
    fn from(v: u16) -> SizedValue {
        SizedValue(v as ValueType, 16)
    }
}

impl From<u32> for SizedValue {
    fn from(v: u32) -> SizedValue {
        SizedValue(v as ValueType, 32)
    }
}

pub struct Variable {
    size: ValueSize,
    /// Log for this variable: `(date, value)`
    log: Vec<(u64, ValueType)>,
}

impl Variable {
    fn new(size: ValueSize) -> Variable {
        Variable {
            size: size,
            log: Vec::new(),
        }
    }

    pub fn size(&self) -> ValueSize {
        self.size
    }

    pub fn log(&self) -> &Vec<(u64, ValueType)> {
        &self.log
    }
}

pub struct Module {
    /// Variables, indexed by name.
    variables: HashMap<&'static str, Variable>,
}

impl Module {
    #[cfg(feature = "trace")]
    fn new() -> Module {
        Module {
            variables: HashMap::new(),
        }
    }

    pub fn variables(&self) -> &HashMap<&'static str, Variable> {
        &self.variables
    }

    pub fn trace<V: Into<SizedValue>>(&mut self,
                                      date: u64,
                                      name: &'static str,
                                      sized_value: V) {
        let SizedValue(value, size) = sized_value.into();

        let var = self.variables.entry(name).or_insert(Variable::new(size));

        if var.size != size {
            panic!("Incoherent size for variable {}: got {} and {}",
                   name, var.size, size);
        }

        if let Some(&(last_date, last_value)) = var.log.last() {
            if last_date >= date {
                panic!("Got out-of-order events for {} ({} >= {})",
                       name, last_date, date);
            }

            if last_value == value {
                // No value change
                return;
            }
        }

        var.log.push((date, value));
    }
}

#[cfg(feature = "trace")]
pub struct Tracer {
    /// Modules, indexed by name
    modules: HashMap<&'static str, Module>,
}

#[cfg(feature = "trace")]
impl Tracer {
    fn new() -> Tracer {
        Tracer {
            modules: HashMap::new(),
        }
    }

    fn module_mut(&mut self, name: &'static str) -> &mut Module {
        self.modules.entry(name).or_insert(Module::new())
    }

    /// Reset the tracer and return the content of the previous trace
    fn remove_trace(&mut self) -> HashMap<&'static str, Module> {
        let mut swap = HashMap::new();

        ::std::mem::swap(&mut self.modules, &mut swap);

        swap
    }
}

/// Global logger instance
#[cfg(feature = "trace")]
lazy_static! {
    static ref LOGGER: ::std::sync::Mutex<Tracer> = {
        ::std::sync::Mutex::new(Tracer::new())
    };
}

#[cfg(feature = "trace")]
pub fn remove_trace() -> HashMap<&'static str, Module> {
    let mut logger = LOGGER.lock().unwrap();

    logger.remove_trace()
}

#[cfg(not(feature = "trace"))]
pub fn remove_trace() -> HashMap<&'static str, Module> {
    HashMap::new()
}

#[cfg(feature = "trace")]
pub fn module_tracer<F>(name: &'static str, f: F)
    where F: FnOnce(&mut Module) {

    let mut logger = LOGGER.lock().unwrap();

    let module = logger.module_mut(name);

    f(module);
}

#[cfg(not(feature = "trace"))]
#[inline(always)]
pub fn module_tracer<F>(_name: &'static str, _f: F)
    where F: FnOnce(&mut Module) {
    // NOP
}

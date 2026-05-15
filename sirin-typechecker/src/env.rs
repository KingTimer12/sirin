use std::collections::HashMap;

use sirin_parser::types::Type;

#[derive(Clone, Debug)]
pub struct Env<'a> {
    scopes: Vec<HashMap<&'a str, Type>>,
    expected_return: Option<Type>,
}

impl<'a> Env<'a> {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            expected_return: None,
        }
    }

    pub fn get_return(&self) -> Option<Type> {
        self.expected_return.clone()
    }

    pub fn set_return(&mut self, return_type: Option<Type>) {
        self.expected_return = return_type
    }

    // abre um novo escopo
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    // fecha o escopo atual — variáveis somem
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    // define no escopo atual
    pub fn define(&mut self, name: &'a str, ty: Type) {
        self.scopes.last_mut().unwrap().insert(name, ty);
    }

    // busca do escopo mais interno para o mais externo
    pub fn get(&self, name: &'a str) -> Option<&Type> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }
}

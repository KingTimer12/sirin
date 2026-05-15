use std::collections::HashMap;

use sirin_parser::types::Type;

#[derive(Clone, Debug, PartialEq)]
pub enum OwnershipState {
    Owned,
    Moved { to: String },
    Borrowed,
}

#[derive(Clone, Debug)]
pub struct Env<'a> {
    scopes: Vec<HashMap<&'a str, Type>>,
    ownership: Vec<HashMap<&'a str, OwnershipState>>,
    expected_return: Option<Type>,
}

impl<'a> Env<'a> {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            ownership: vec![HashMap::new()],
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
        self.ownership.push(HashMap::new());
    }

    // fecha o escopo atual — variáveis somem
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
        self.ownership.pop();
    }

    // define no escopo atual
    pub fn define(&mut self, name: &'a str, ty: Type) {
        self.scopes.last_mut().unwrap().insert(name, ty);
        self.ownership.last_mut().unwrap().insert(name, OwnershipState::Owned);
    }

    // busca do escopo mais interno para o mais externo
    pub fn get(&self, name: &'a str) -> Option<&Type> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    pub fn get_ownership(&self, name: &'a str) -> Option<&OwnershipState> {
        self.ownership.iter().rev().find_map(|scope| scope.get(name))
    }

    // marca variável como movida para `to`; retorna Some(scope_idx) se encontrada
    pub fn mark_moved(&mut self, name: &'a str, to: String) -> bool {
        for scope in self.ownership.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name, OwnershipState::Moved { to });
                return true;
            }
        }
        false
    }
}

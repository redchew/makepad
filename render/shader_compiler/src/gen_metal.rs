use crate::ast::*;
use crate::env::VarKind;
use crate::ident::Ident;
use crate::lit::{Lit, TyLit};
use crate::span::Span;
use crate::swizzle::Swizzle;
use crate::ty::Ty;
use std::cell::{Cell, RefCell};
use std::fmt::Write;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug)]
pub enum ShaderKind {
    Vertex,
    Pixel,
}

pub fn generate(shader_ast: &ShaderAst) -> String {
    let mut string = String::new();
    ShaderGenerator {
        string: &mut string,
        shader_ast,
    }
    .generate_shader();
    string
}

#[derive(Debug)]
struct ShaderGenerator<'a> {
    string: &'a mut String,
    shader_ast: &'a ShaderAst,
}

impl<'a> ShaderGenerator<'a> {
    fn generate_shader(&mut self) {
        /*
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Struct(decl) => self.generate_struct_decl(decl),
                _ => {}
            }
        }
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Const(decl) => self.generate_const_decl(decl),
                _ => {}
            }
        }*/
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Uniform(decl) => self.generate_uniform_decl(decl),
                _ => {}
            }
        }

        let mut merge_cons = HashSet::new();
        for (ty_lit, param_tys) in self.shader_ast.find_fn_decl(Ident::new("vertex"))
            .unwrap().cons_deps.borrow().as_ref().unwrap()
        {
            merge_cons.insert((ty_lit.clone(), param_tys.clone()));
        }
        for (ty_lit, param_tys) in self.shader_ast.find_fn_decl(Ident::new("pixel"))
            .unwrap().cons_deps.borrow().as_ref().unwrap()
        {
            merge_cons.insert((ty_lit.clone(), param_tys.clone()));
        }
        
        for (ty_lit, param_tys) in merge_cons {
            self.generate_cons(ty_lit, &param_tys);
        }
        
        // todo merge these
        self.generate_fn_defs(Ident::new("vertex"), ShaderKind::Vertex);
        self.generate_fn_defs(Ident::new("pixel"), ShaderKind::Pixel);
        
        let vertex_decl = self.shader_ast.find_fn_decl(Ident::new("vertex")).unwrap();
        
        self.generate_attributes_struct();
        self.generate_instances_struct();
        self.generate_varyings_struct();
        
        writeln!(self.string, "void main() {{").unwrap();
        writeln!(self.string, "    _m_Attributes _m_attributes;").unwrap();
        writeln!(self.string, "    _m_Instances _m_instances;").unwrap();
        writeln!(self.string, "    _m_Varyings _m_varyings;").unwrap();
        write!(self.string, "    gl_Position = vertex(").unwrap();
        let mut sep = "";
        for &ident in vertex_decl.uniform_block_deps.borrow().as_ref().unwrap() {
            write!(self.string, "{}_m_{1}_uniforms", sep, ident).unwrap();
            sep = ", ";
        }
        if !vertex_decl
            .attribute_deps
            .borrow()
            .as_ref()
            .unwrap()
            .is_empty()
        {
            write!(self.string, "{}_m_attributes", sep).unwrap();
            sep = ", ";
        }
        if vertex_decl.has_out_varying_deps.get().unwrap() {
            write!(self.string, "{}_m_varyings", sep).unwrap();
        }
        writeln!(self.string, ");").unwrap();
        writeln!(self.string, "}}").unwrap();
        
        // lets generate vertex and pixelshaders.
    }
    
    fn generate_struct_decl(&mut self, decl: &StructDecl) {
        write!(self.string, "struct {} {{", decl.ident).unwrap();
        if !decl.fields.is_empty() {
            writeln!(self.string).unwrap();
            for field in &decl.fields {
                write!(self.string, "    ").unwrap();
                write_ident_and_ty(
                    &mut self.string,
                    field.ident,
                    field.ty_expr.ty.borrow().as_ref().unwrap(),
                );
                writeln!(self.string, ";").unwrap();
            }
        }
        writeln!(self.string, "}};").unwrap();
    }
    
    fn generate_const_decl(&mut self, decl: &ConstDecl) {
        write!(self.string, "const ").unwrap();
        write_ident_and_ty(
            &mut self.string,
            decl.ident,
            decl.ty_expr.ty.borrow().as_ref().unwrap(),
        );
        write!(self.string, " = ").unwrap();
        self.generate_expr(&decl.expr, ShaderKind::Vertex);
        writeln!(self.string, ";").unwrap();
    }
    
    fn generate_uniform_decl(&mut self, decl: &UniformDecl) {
        write!(self.string, "uniform ").unwrap();
        write_ident_and_ty(
            self.string,
            decl.ident,
            decl.ty_expr.ty.borrow().as_ref().unwrap(),
        );
        writeln!(self.string, ";").unwrap();
    }
    
    fn generate_cons(&mut self, ty_lit: TyLit, param_tys: &[Ty]) {
        write!(self.string, "{0} _m_{0}", ty_lit).unwrap();
        for param_ty in param_tys {
            write!(self.string, "_{}", param_ty).unwrap();
        }
        write!(self.string, "(").unwrap();
        let mut sep = "";
        if param_tys.len() == 1 {
            write_ident_and_ty(&mut self.string, Ident::new("x"), &param_tys[0])
        } else {
            for (index, param_ty) in param_tys.iter().enumerate() {
                write!(self.string, "{}", sep).unwrap();
                write_ident_and_ty(
                    &mut self.string,
                    Ident::new(format!("x{}", index)),
                    param_ty,
                );
                sep = ", ";
            }
        }
        writeln!(self.string, ") {{").unwrap();
        write!(self.string, "    {}(", ty_lit).unwrap();
        let ty = ty_lit.to_ty();
        if param_tys.len() == 1 {
            let param_ty = &param_tys[0];
            match param_ty {
                Ty::Bool | Ty::Int | Ty::Float => {
                    let mut sep = "";
                    for _ in 0..ty.size() {
                        write!(self.string, "{}x", sep).unwrap();
                        sep = ", ";
                    }
                }
                Ty::Mat2 | Ty::Mat3 | Ty::Mat4 => {
                    let target_size = match ty {
                        Ty::Mat2 => 2,
                        Ty::Mat3 => 3,
                        Ty::Mat4 => 4,
                        _ => panic!(),
                    };
                    let source_size = match param_ty {
                        Ty::Mat2 => 2,
                        Ty::Mat3 => 3,
                        Ty::Mat4 => 4,
                        _ => panic!(),
                    };
                    let mut sep = "";
                    for column_index in 0..target_size {
                        for row_index in 0..target_size {
                            if row_index < source_size && column_index < source_size {
                                write!(self.string, "{}x[{}][{}]", sep, column_index, row_index)
                                    .unwrap();
                            } else {
                                write!(
                                    self.string,
                                    "{}{}",
                                    sep,
                                    if column_index == row_index {1.0} else {0.0}
                                )
                                    .unwrap();
                            }
                            sep = ", ";
                        }
                    }
                }
                _ => panic!(),
            }
        } else {
            let mut sep = "";
            for (index_0, param_ty) in param_tys.iter().enumerate() {
                if param_ty.size() == 1 {
                    write!(self.string, "{}x{}", sep, index_0).unwrap();
                    sep = ", ";
                } else {
                    for index_1 in 0..param_ty.size() {
                        write!(self.string, "{}x{}[{}]", sep, index_0, index_1).unwrap();
                        sep = ", ";
                    }
                }
            }
        }
        writeln!(self.string, ")").unwrap();
        writeln!(self.string, "}}").unwrap();
    }
    
    fn generate_fn_defs(&mut self, ident: Ident, kind:ShaderKind) {
        let decl = self.shader_ast.find_fn_decl(ident).unwrap();
        for &callee in decl.callees.borrow().as_ref().unwrap().iter() {
            self.generate_fn_defs(callee, kind);
        }
        FnDefGenerator {
            string: self.string,
            indent_level: 0,
            kind: kind,
            shader_ast: &self.shader_ast,
            decl,
        }
        .generate_fn_def()
    }
    
    fn generate_expr(&mut self, expr: &Expr, kind:ShaderKind) {
        ExprGenerator {
            string: self.string,
            kind: kind,
            shader_ast: self.shader_ast,
        }
        .generate_expr(expr)
    }
    
    fn generate_vertex_shader(&mut self) {
      
    }
    
    fn generate_pixel_shader(&mut self) {
        let pixel_decl = self.shader_ast.find_fn_decl(Ident::new("pixel")).unwrap();
        let total_packed_varying_size = self.compute_total_packed_varying_size();
        self.generate_packed_varyings(total_packed_varying_size);
        writeln!(self.string, "void main() {{").unwrap();
        writeln!(self.string, "    _m_Varyings _m_varyings;").unwrap();
        self.generate_unpack_varyings(total_packed_varying_size);
        write!(self.string, "    gl_FragColor = pixel(").unwrap();
        let mut sep = "";
        for &ident in pixel_decl.uniform_block_deps.borrow().as_ref().unwrap() {
            write!(self.string, "{}_m_{1}_uniforms", sep, ident).unwrap();
            sep = ", ";
        }
        if !pixel_decl
            .attribute_deps
            .borrow()
            .as_ref()
            .unwrap()
            .is_empty()
            || pixel_decl.has_out_varying_deps.get().unwrap()
        {
            write!(self.string, "{}_m_varyings", sep).unwrap();
        }
        writeln!(self.string, ");").unwrap();
        writeln!(self.string, "}}").unwrap();
    }
    
    fn generate_attributes_struct(&mut self) {
        writeln!(self.string, "struct _m_Attributes {{").unwrap();
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Attribute(decl) => {
                    write!(self.string, "    ").unwrap();
                    write_ident_and_ty(
                        &mut self.string,
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap()
                    );
                    writeln!(self.string, ";").unwrap();
                }
                _ => {}
            }
        }
        writeln!(self.string, "}};").unwrap();
    }
    
    fn generate_instances_struct(&mut self) {
        writeln!(self.string, "struct _m_Instances {{").unwrap();
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Instance(decl) => {
                    write!(self.string, "    ").unwrap();
                    write_ident_and_ty(
                        &mut self.string,
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap()
                    );
                    writeln!(self.string, ";").unwrap();
                }
                _ => {}
            }
        }
        writeln!(self.string, "}};").unwrap();
    }
    
    fn generate_varyings_struct(&mut self) {
        let pixel_decl = self.shader_ast.find_fn_decl(Ident::new("pixel")).unwrap();
        writeln!(self.string, "struct _m_Varyings {{").unwrap();
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Attribute(decl)
                if pixel_decl
                    .attribute_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .contains(&decl.ident) =>
                {
                    write!(self.string, "    ").unwrap();
                    write_ident_and_ty(
                        &mut self.string,
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap()
                    );
                    writeln!(self.string, ";").unwrap();
                }
                Decl::Instance(decl)
                if pixel_decl
                    .instance_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .contains(&decl.ident) =>
                {
                    write!(self.string, "    ").unwrap();
                    write_ident_and_ty(
                        &mut self.string,
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap()
                    );
                    writeln!(self.string, ";").unwrap();
                }
                Decl::Varying(decl) => {
                    write!(self.string, "    ").unwrap();
                    write_ident_and_ty(
                        &mut self.string,
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap()
                    );
                    writeln!(self.string, ";").unwrap();
                }
                _ => {}
            }
        }
        writeln!(self.string, "}};").unwrap();
    }
    
    fn compute_total_packed_attribute_size(&mut self) -> usize {
        let mut total_packed_attribute_size = 0;
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Attribute(decl) => {
                    total_packed_attribute_size +=
                    decl.ty_expr.ty.borrow().as_ref().unwrap().size();
                }
                Decl::Instance(decl) => {
                    total_packed_attribute_size +=
                    decl.ty_expr.ty.borrow().as_ref().unwrap().size();
                }
                _ => {}
            }
        }
        total_packed_attribute_size
    }
    
    fn generate_packed_attributes(&mut self, total_packed_attribute_size: usize) {
        let mut remaining_packed_attribute_size = total_packed_attribute_size;
        let mut current_packed_attribute_index = 0;
        loop {
            let current_packed_attribute_size = remaining_packed_attribute_size.min(4);
            writeln!(
                self.string,
                "attribute {} _m_packed_attribute_{};",
                match current_packed_attribute_size {
                    0 => break,
                    1 => "float",
                    2 => "vec2",
                    3 => "vec3",
                    4 => "vec4",
                    _ => panic!(),
                },
                current_packed_attribute_index,
            )
                .unwrap();
            remaining_packed_attribute_size -= current_packed_attribute_size;
            current_packed_attribute_index += 1;
        }
    }
    
    fn compute_total_packed_varying_size(&mut self) -> usize {
        let pixel_decl = self.shader_ast.find_fn_decl(Ident::new("pixel")).unwrap();
        let mut total_packed_varying_size = 0;
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Attribute(decl)
                if pixel_decl
                    .attribute_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .contains(&decl.ident) =>
                {
                    total_packed_varying_size += decl.ty_expr.ty.borrow().as_ref().unwrap().size();
                }
                Decl::Instance(decl)
                if pixel_decl
                    .instance_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .contains(&decl.ident) =>
                {
                    total_packed_varying_size += decl.ty_expr.ty.borrow().as_ref().unwrap().size();
                }
                Decl::Varying(decl) => {
                    total_packed_varying_size += decl.ty_expr.ty.borrow().as_ref().unwrap().size();
                }
                _ => {}
            }
        }
        total_packed_varying_size
    }
    
    fn generate_packed_varyings(&mut self, total_packed_varying_size: usize) {
        let mut remaining_packed_varying_size = total_packed_varying_size;
        let mut current_packed_varying_index = 0;
        loop {
            let current_packed_varying_size = remaining_packed_varying_size.min(4);
            writeln!(
                self.string,
                "varying {} _mpsc_packed_varying_{};",
                match current_packed_varying_size {
                    0 => break,
                    1 => "float",
                    2 => "vec2",
                    3 => "vec3",
                    4 => "vec4",
                    _ => panic!(),
                },
                current_packed_varying_index,
            )
                .unwrap();
            remaining_packed_varying_size -= current_packed_varying_size;
            current_packed_varying_index += 1;
        }
    }
    
    fn generate_unpack_attributes(&mut self, total_packed_attribute_size: usize) {
        let mut remaining_packed_attribute_size = total_packed_attribute_size;
        let mut current_packed_attribute_index = 0;
        let mut current_packed_attribute_size = remaining_packed_attribute_size.min(4);
        let mut current_packed_attribute_offset = 0;
        let mut unpack_attribute = {
            let string = &mut self.string;
            move | ident: Ident,
            ty: &Ty,
            target_name: &str | {
                let current_attribute_size = ty.size();
                let mut current_attribute_offset = 0;
                while current_attribute_offset < current_attribute_size {
                    let count = (current_packed_attribute_size - current_packed_attribute_offset)
                        .min(current_attribute_size - current_attribute_offset);
                    write!(string, "    {}.{}", target_name, ident).unwrap();
                    if current_attribute_size > 1 {
                        write!(
                            string,
                            ".{}",
                            Swizzle::from_range(
                                current_attribute_offset,
                                current_attribute_offset + count
                            )
                        )
                            .unwrap();
                    }
                    write!(
                        string,
                        " = _m_packed_attribute_{}",
                        current_packed_attribute_index
                    )
                        .unwrap();
                    if current_packed_attribute_size > 1 {
                        write!(
                            string,
                            ".{}",
                            Swizzle::from_range(
                                current_packed_attribute_offset,
                                current_packed_attribute_offset + count
                            )
                        )
                            .unwrap();
                    }
                    writeln!(string, ";").unwrap();
                    current_attribute_offset += count;
                    current_packed_attribute_offset += count;
                    if current_packed_attribute_offset == current_packed_attribute_size {
                        remaining_packed_attribute_size -= current_packed_attribute_size;
                        current_packed_attribute_index += 1;
                        current_packed_attribute_size = remaining_packed_attribute_size.min(4);
                        current_packed_attribute_offset = 0;
                    }
                }
            }
        };
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Attribute(decl) => {
                    unpack_attribute(
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap(),
                        "_m_attributes"
                    );
                },
                Decl::Instance(decl) => {
                    unpack_attribute(
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap(),
                        "_m_instances"
                    );
                }
                _ => {}
            }
        }
    }
    
    fn generate_pack_varyings(&mut self, total_packed_varying_size: usize) {
        let pixel_decl = self.shader_ast.find_fn_decl(Ident::new("pixel")).unwrap();
        let mut remaining_packed_varying_size = total_packed_varying_size;
        let mut current_packed_varying_index = 0;
        let mut current_packed_varying_size = remaining_packed_varying_size.min(4);
        let mut current_packed_varying_offset = 0;
        let mut pack_varying = {
            let string = &mut self.string;
            move | ident: Ident,
            ty: &Ty,
            source_name: &str | {
                let current_varying_size = ty.size();
                let mut current_varying_offset = 0;
                while current_varying_offset < current_varying_size {
                    let count = (current_packed_varying_size - current_packed_varying_offset)
                        .min(current_varying_size - current_varying_offset);
                    write!(
                        string,
                        "    _m_packed_varying_{}",
                        current_packed_varying_index
                    )
                        .unwrap();
                    if current_packed_varying_size > 1 {
                        write!(
                            string,
                            ".{}",
                            Swizzle::from_range(
                                current_packed_varying_offset,
                                current_packed_varying_offset + count
                            )
                        )
                            .unwrap();
                    }
                    write!(string, " = {}.{}", source_name, ident).unwrap();
                    if current_varying_size > 1 {
                        write!(
                            string,
                            ".{}",
                            Swizzle::from_range(
                                current_varying_offset,
                                current_varying_offset + count
                            )
                        )
                            .unwrap();
                    }
                    writeln!(string, ";").unwrap();
                    current_packed_varying_offset += count;
                    current_varying_offset += count;
                    if current_packed_varying_offset == current_packed_varying_size {
                        remaining_packed_varying_size -= current_packed_varying_size;
                        current_packed_varying_index += 1;
                        current_packed_varying_size = remaining_packed_varying_size.min(4);
                        current_packed_varying_offset = 0;
                    }
                }
            }
        };
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Attribute(decl)
                if pixel_decl
                    .attribute_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .contains(&decl.ident) =>
                {
                    pack_varying(
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap(),
                        "_mpsc_attributes",
                    );
                }
                Decl::Instance(decl)
                if pixel_decl
                    .instance_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .contains(&decl.ident) =>
                {
                    pack_varying(
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap(),
                        "_mpsc_instances",
                    );
                }
                Decl::Varying(decl) => {
                    pack_varying(
                        decl.ident,
                        decl.ty_expr.ty.borrow().as_ref().unwrap(),
                        "_mpsc_varyings",
                    );
                }
                _ => {}
            }
        }
    }
    
    fn generate_unpack_varyings(&mut self, total_packed_varying_size: usize) {
        let fragment_decl = self.shader_ast.find_fn_decl(Ident::new("pixel")).unwrap();
        let mut remaining_packed_varying_size = total_packed_varying_size;
        let mut current_packed_varying_index = 0;
        let mut current_packed_varying_size = remaining_packed_varying_size.min(4);
        let mut current_packed_varying_offset = 0;
        let mut unpack_varying = {
            let string = &mut self.string;
            move | ident: Ident,
            ty: &Ty | {
                let current_varying_size = ty.size();
                let mut current_varying_offset = 0;
                while current_varying_offset < current_varying_size {
                    let count = (current_packed_varying_size - current_packed_varying_offset)
                        .min(current_varying_size - current_varying_offset);
                    write!(string, "    _m_varyings.{}", ident).unwrap();
                    if current_varying_size > 1 {
                        write!(
                            string,
                            ".{}",
                            Swizzle::from_range(
                                current_varying_offset,
                                current_varying_offset + count
                            )
                        )
                            .unwrap();
                    }
                    write!(
                        string,
                        " = _m_packed_varying_{}",
                        current_packed_varying_index
                    )
                        .unwrap();
                    if current_packed_varying_size > 1 {
                        write!(
                            string,
                            ".{}",
                            Swizzle::from_range(
                                current_packed_varying_offset,
                                current_packed_varying_offset + count
                            )
                        )
                            .unwrap();
                    }
                    writeln!(string, ";").unwrap();
                    current_varying_offset += count;
                    current_packed_varying_offset += count;
                    if current_packed_varying_offset == current_packed_varying_size {
                        remaining_packed_varying_size -= current_packed_varying_size;
                        current_packed_varying_index += 1;
                        current_packed_varying_size = remaining_packed_varying_size.min(4);
                        current_packed_varying_offset = 0;
                    }
                }
            }
        };
        for decl in &self.shader_ast.decls {
            match decl {
                Decl::Attribute(decl)
                if fragment_decl
                    .attribute_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .contains(&decl.ident) =>
                {
                    unpack_varying(decl.ident, decl.ty_expr.ty.borrow().as_ref().unwrap());
                }
                Decl::Instance(decl)
                if fragment_decl
                    .instance_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .contains(&decl.ident) =>
                {
                    unpack_varying(decl.ident, decl.ty_expr.ty.borrow().as_ref().unwrap());
                }
                Decl::Varying(decl) => {
                    unpack_varying(decl.ident, decl.ty_expr.ty.borrow().as_ref().unwrap());
                }
                _ => {}
            }
        }
    }
}

#[derive(Debug)]
struct FnDefGenerator<'a> {
    string: &'a mut String,
    indent_level: usize,
    kind: ShaderKind,
    shader_ast: &'a ShaderAst,
    decl: &'a FnDecl,
}

impl<'a> FnDefGenerator<'a> {
    fn generate_fn_def(&mut self) {
        write_ident_and_ty(
            &mut self.string,
            self.decl.ident,
            self.decl.return_ty.borrow().as_ref().unwrap(),
        );
        write!(self.string, "(").unwrap();
        let mut sep = "";
        for param in &self.decl.params {
            write!(self.string, "{}", sep).unwrap();
            write_ident_and_ty(
                &mut self.string,
                param.ident,
                param.ty_expr.ty.borrow().as_ref().unwrap(),
            );
            sep = ", ";
        }
        for &ident in self.decl.uniform_block_deps.borrow().as_ref().unwrap() {
            write!(
                self.string,
                "{}_m_{1}_Uniforms _m_{1}_uniforms",
                sep,
                ident
            )
                .unwrap();
            sep = ", ";
        }
        match self.kind {
            ShaderKind::Vertex => {
                if !self
                .decl
                    .attribute_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .is_empty()
                {
                    write!(self.string, "{}_m_Attributes _m_attributes", sep).unwrap();
                    sep = ", ";
                }
                if self.decl.has_out_varying_deps.get().unwrap() {
                    write!(self.string, "{}out _m_Varyings _m_varyings", sep).unwrap();
                }
            }
            ShaderKind::Pixel => {
                if !self
                .decl
                    .attribute_deps
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .is_empty()
                    || self.decl.has_in_varying_deps.get().unwrap()
                {
                    write!(self.string, "{}_m_Varyings _m_varyings", sep).unwrap();
                }
            }
        }
        write!(self.string, ") ").unwrap();
        self.generate_block(&self.decl.block);
        writeln!(self.string).unwrap();
    }
    
    fn generate_block(&mut self, block: &Block) {
        write!(self.string, "{{").unwrap();
        if !block.stmts.is_empty() {
            writeln!(self.string).unwrap();
            self.indent_level += 1;
            for stmt in &block.stmts {
                self.generate_stmt(stmt);
            }
            self.indent_level -= 1;
        }
        write!(self.string, "}}").unwrap()
    }
    
    fn generate_stmt(&mut self, stmt: &Stmt) {
        self.write_indent();
        match *stmt {
            Stmt::Break {
                span
            } => self.generate_break_stmt(span),
            Stmt::Continue {
                span
            } => self.generate_continue_stmt(span),
            Stmt::For {
                span,
                ident,
                ref from_expr,
                ref to_expr,
                ref step_expr,
                ref block,
            } => self.generate_for_stmt(span, ident, from_expr, to_expr, step_expr, block),
            Stmt::If {
                span,
                ref expr,
                ref block_if_true,
                ref block_if_false,
            } => self.generate_if_stmt(span, expr, block_if_true, block_if_false),
            Stmt::Let {
                span,
                ref ty,
                ident,
                ref ty_expr,
                ref expr,
            } => self.generate_let_stmt(span, ty, ident, ty_expr, expr),
            Stmt::Return {
                span,
                ref expr
            } => self.generate_return_stmt(span, expr),
            Stmt::Block {
                span,
                ref block
            } => self.generate_block_stmt(span, block),
            Stmt::Expr {
                span,
                ref expr
            } => self.generate_expr_stmt(span, expr),
        }
    }
    
    fn generate_break_stmt(&mut self, _span: Span) {
        writeln!(self.string, "break;").unwrap();
    }
    
    fn generate_continue_stmt(&mut self, _span: Span) {
        writeln!(self.string, "continue;").unwrap();
    }
    
    fn generate_for_stmt(
        &mut self,
        _span: Span,
        ident: Ident,
        from_expr: &Expr,
        to_expr: &Expr,
        step_expr: &Option<Expr>,
        block: &Block,
    ) {
        let from = from_expr.val.borrow().as_ref().unwrap().to_int().unwrap();
        let to = to_expr.val.borrow().as_ref().unwrap().to_int().unwrap();
        let step = if let Some(step_expr) = step_expr {
            step_expr.val.borrow().as_ref().unwrap().to_int().unwrap()
        } else if from < to {
            1
        } else {
            -1
        };
        write!(
            self.string,
            "for (int {0} = {1}; {0} {2} {3}; {0} {4} {5} ",
            ident,
            if from <= to {from} else {from - 1},
            if from <= to {"<"} else {">="},
            to,
            if step > 0 {"+="} else {"-="},
            step.abs()
        )
            .unwrap();
        self.generate_block(block);
        writeln!(self.string).unwrap();
    }
    
    fn generate_if_stmt(
        &mut self,
        _span: Span,
        expr: &Expr,
        block_if_true: &Block,
        block_if_false: &Option<Box<Block>>,
    ) {
        write!(self.string, "if (").unwrap();
        self.generate_expr(expr);
        write!(self.string, " ").unwrap();
        self.generate_block(block_if_true);
        if let Some(block_if_false) = block_if_false {
            self.generate_block(block_if_false);
        }
        writeln!(self.string).unwrap();
    }
    
    fn generate_let_stmt(
        &mut self,
        _span: Span,
        ty: &RefCell<Option<Ty>>,
        ident: Ident,
        _ty_expr: &Option<TyExpr>,
        expr: &Option<Expr>,
    ) {
        write_ident_and_ty(&mut self.string, ident, ty.borrow().as_ref().unwrap());
        if let Some(expr) = expr {
            write!(self.string, " = ").unwrap();
            self.generate_expr(expr);
        }
        writeln!(self.string, ";").unwrap();
    }
    
    fn generate_return_stmt(&mut self, _span: Span, expr: &Option<Expr>) {
        write!(self.string, "return").unwrap();
        if let Some(expr) = expr {
            write!(self.string, " ").unwrap();
            self.generate_expr(expr);
        }
        writeln!(self.string, ";").unwrap();
    }
    
    fn generate_block_stmt(&mut self, _span: Span, block: &Block) {
        self.generate_block(block);
        writeln!(self.string).unwrap();
    }
    
    fn generate_expr_stmt(&mut self, _span: Span, expr: &Expr) {
        self.generate_expr(expr);
        writeln!(self.string, ";").unwrap();
    }
    
    fn generate_expr(&mut self, expr: &Expr) {
        ExprGenerator {
            string: self.string,
            kind: self.kind,
            shader_ast: self.shader_ast,
        }
        .generate_expr(expr)
    }
    
    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            write!(self.string, "    ").unwrap();
        }
    }
}

struct ExprGenerator<'a> {
    string: &'a mut String,
    kind: ShaderKind,
    shader_ast: &'a ShaderAst,
}

impl<'a> ExprGenerator<'a> {
    fn generate_expr(&mut self, expr: &Expr) {
        match expr.kind {
            ExprKind::Cond {
                span,
                ref expr,
                ref expr_if_true,
                ref expr_if_false,
            } => self.generate_cond_expr(span, expr, expr_if_true, expr_if_false),
            ExprKind::Bin {
                span,
                op,
                ref left_expr,
                ref right_expr,
            } => self.generate_bin_expr(span, op, left_expr, right_expr),
            ExprKind::Un {
                span,
                op,
                ref expr
            } => self.generate_un_expr(span, op, expr),
            ExprKind::MethodCall {
                span,
                ident,
                ref arg_exprs
            } => self.generate_method_call_expr(span, ident, arg_exprs),
            ExprKind::Field {
                span,
                ref expr,
                field_ident,
            } => self.generate_field_expr(span, expr, field_ident),
            ExprKind::Index {
                span,
                ref expr,
                ref index_expr,
            } => self.generate_index_expr(span, expr, index_expr),
            ExprKind::Call {
                span,
                ident,
                ref arg_exprs,
            } => self.generate_call_expr(span, ident, arg_exprs),
            ExprKind::MacroCall {
                span,
                ident,
                ref arg_exprs,
                ..
            } => self.generate_macro_call_expr(span, ident, arg_exprs),
            ExprKind::ConsCall {
                span,
                ty_lit,
                ref arg_exprs,
            } => self.generate_cons_call_expr(span, ty_lit, arg_exprs),
            ExprKind::Var {
                span,
                ref is_lvalue,
                ref kind,
                ident,
            } => self.generate_var_expr(span, is_lvalue, kind, ident),
            ExprKind::Lit {span, lit} => self.generate_lit_expr(span, lit),
        }
    }
    
    fn generate_cond_expr(
        &mut self,
        _span: Span,
        expr: &Expr,
        expr_if_true: &Expr,
        expr_if_false: &Expr
    ) {
        write!(self.string, "(").unwrap();
        self.generate_expr(expr);
        write!(self.string, " ? ").unwrap();
        self.generate_expr(expr_if_true);
        write!(self.string, " : ").unwrap();
        self.generate_expr(expr_if_false);
        write!(self.string, ")").unwrap();
    }
    
    fn generate_bin_expr(
        &mut self,
        _span: Span,
        op: BinOp,
        left_expr: &Expr,
        right_expr: &Expr
    ) {
        write!(self.string, "(").unwrap();
        self.generate_expr(left_expr);
        write!(self.string, " {} ", op).unwrap();
        self.generate_expr(right_expr);
        write!(self.string, ")").unwrap();
    }
    
    fn generate_un_expr(&mut self, _span: Span, op: UnOp, expr: &Expr) {
        write!(self.string, "{}", op).unwrap();
        self.generate_expr(expr);
    }
    
    fn generate_method_call_expr(
        &mut self,
        span: Span,
        ident: Ident,
        arg_exprs: &[Expr]
    ) {
        match arg_exprs[0].ty.borrow().as_ref().unwrap() {
            Ty::Struct {ident: struct_ident} => {
                self.generate_call_expr(
                    span,
                    Ident::new(format!("{}::{}", struct_ident, ident)),
                    arg_exprs
                );
            },
            _ => panic!()
        }
    }
    
    fn generate_field_expr(&mut self, _span: Span, expr: &Expr, field_ident: Ident) {
        self.generate_expr(expr);
        write!(self.string, ".{}", field_ident).unwrap();
    }
    
    fn generate_index_expr(&mut self, _span: Span, expr: &Expr, index_expr: &Expr) {
        self.generate_expr(expr);
        write!(self.string, "[").unwrap();
        self.generate_expr(index_expr);
        write!(self.string, "]").unwrap();
    }
    
    fn generate_call_expr(&mut self, _span: Span, ident: Ident, arg_exprs: &[Expr]) {
        write!(self.string, "{}(", ident).unwrap();
        let mut sep = "";
        for arg_expr in arg_exprs {
            write!(self.string, "{}", sep).unwrap();
            self.generate_expr(arg_expr);
            sep = ", ";
        }
        if let Some(decl) = self.shader_ast.find_fn_decl(ident) {
            for &ident in decl.uniform_block_deps.borrow().as_ref().unwrap() {
                write!(self.string, "{}_m_{1}_uniforms", sep, ident).unwrap();
                sep = ", ";
            }
            match self.kind {
                ShaderKind::Vertex => {
                    if !decl.attribute_deps.borrow().as_ref().unwrap().is_empty() {
                        write!(self.string, "{}_m_attributes", sep).unwrap();
                        sep = ", ";
                    }
                    if !decl.instance_deps.borrow().as_ref().unwrap().is_empty() {
                        write!(self.string, "{}_m_instances", sep).unwrap();
                        sep = ", ";
                    }
                    if decl.has_out_varying_deps.get().unwrap() {
                        write!(self.string, "{}_m_varyings", sep).unwrap();
                    }
                }
                ShaderKind::Pixel => {
                    if !decl.attribute_deps.borrow().as_ref().unwrap().is_empty()
                        || decl.has_in_varying_deps.get().unwrap()
                    {
                        write!(self.string, "{}_m_varyings", sep).unwrap();
                    }
                }
            }
        }
        write!(self.string, ")").unwrap();
    }
    
    fn generate_macro_call_expr(&mut self, _span: Span, _ident: Ident, _arg_exprs: &[Expr]) {
        write!(self.string, "").unwrap();
    }
    
    
    fn generate_cons_call_expr(&mut self, _span: Span, ty_lit: TyLit, arg_exprs: &[Expr]) {
        write!(self.string, "_mpsc_{}", ty_lit).unwrap();
        for arg_expr in arg_exprs {
            write!(self.string, "_{}", arg_expr.ty.borrow().as_ref().unwrap()).unwrap();
        }
        write!(self.string, "(").unwrap();
        let mut sep = "";
        for arg_expr in arg_exprs {
            write!(self.string, "{}", sep).unwrap();
            self.generate_expr(arg_expr);
            sep = ", ";
        }
        write!(self.string, ")").unwrap();
    }
    
    fn generate_var_expr(
        &mut self,
        _span: Span,
        _is_lvalue: &Cell<Option<bool>>,
        kind: &Cell<Option<VarKind>>,
        ident: Ident,
    ) {
        match kind.get().unwrap() {
            VarKind::Attribute => match self.kind {
                ShaderKind::Vertex => write!(self.string, "_m_attributes.").unwrap(),
                ShaderKind::Pixel => write!(self.string, "_m_varyings.").unwrap(),
            },
            VarKind::Const => {}
            VarKind::Instance => match self.kind {
                ShaderKind::Vertex => write!(self.string, "_m_instances.").unwrap(),
                ShaderKind::Pixel => write!(self.string, "_m_varyings.").unwrap(),
            },
            VarKind::Local => {}
            VarKind::Texture => {}
            VarKind::Varying => write!(self.string, "_m_varyings.").unwrap(),
            VarKind::Uniform => {}
        }
        write!(self.string, "{}", ident).unwrap()
    }
    
    fn generate_lit_expr(&mut self, _span: Span, lit: Lit) {
        write!(self.string, "{}", lit).unwrap();
    }
}

fn write_ident_and_ty(string: &mut String, ident: Ident, ty: &Ty) {
    match *ty {
        Ty::Void => write!(string, "void {}", ident).unwrap(),
        Ty::Bool => write!(string, "bool {}", ident).unwrap(),
        Ty::Int => write!(string, "int {}", ident).unwrap(),
        Ty::Float => write!(string, "float {}", ident).unwrap(),
        Ty::Bvec2 => write!(string, "bvec2 {}", ident).unwrap(),
        Ty::Bvec3 => write!(string, "bvec3 {}", ident).unwrap(),
        Ty::Bvec4 => write!(string, "bvec4 {}", ident).unwrap(),
        Ty::Ivec2 => write!(string, "ivec2 {}", ident).unwrap(),
        Ty::Ivec3 => write!(string, "ivec3 {}", ident).unwrap(),
        Ty::Ivec4 => write!(string, "ivec4 {}", ident).unwrap(),
        Ty::Vec2 => write!(string, "vec2 {}", ident).unwrap(),
        Ty::Vec3 => write!(string, "vec3 {}", ident).unwrap(),
        Ty::Vec4 => write!(string, "vec4 {}", ident).unwrap(),
        Ty::Mat2 => write!(string, "mat2 {}", ident).unwrap(),
        Ty::Mat3 => write!(string, "mat3 {}", ident).unwrap(),
        Ty::Mat4 => write!(string, "mat4 {}", ident).unwrap(),
        Ty::Texture2d => panic!(),
        Ty::Array {ref elem_ty, len} => {
            write_ident_and_ty(string, ident, elem_ty);
            write!(string, "[{}]", len).unwrap();
        }
        Ty::Struct {
            ident: struct_ident,
        } => write!(string, "{} {}", struct_ident, ident).unwrap(),
    }
}
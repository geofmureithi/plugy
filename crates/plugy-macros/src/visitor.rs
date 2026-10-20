use syn::{
    parse_quote,
    visit_mut::{self, VisitMut}, Path,
    Type,
};

/// Guest side: `Self::Assoc` -> `<Concrete as Trait>::Assoc`, bare `Self` -> `Concrete`.
pub struct GuestSelfRewriter<'a> {
    pub(crate) self_ty: &'a Type,
    pub(crate) trait_path: &'a Path,
}

impl VisitMut for GuestSelfRewriter<'_> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Type::Path(tp) = ty {
            if tp.qself.is_none() {
                let segs = &tp.path.segments;
                if segs.len() == 1 && segs[0].ident == "Self" {
                    *ty = self.self_ty.clone();
                    return;
                }
                if segs.len() == 2 && segs[0].ident == "Self" {
                    let (self_ty, trait_path) = (self.self_ty, self.trait_path);
                    let name = &segs[1].ident;
                    *ty = parse_quote!(<#self_ty as #trait_path>::#name);
                    return;
                }
            }
        }
        visit_mut::visit_type_mut(self, ty);
    }
}

/// Host side: `Self::Assoc` -> generic parameter `Assoc` (recording which were used).
pub(crate) struct HostAssocRewriter<'a> {
    pub(crate) known: &'a [syn::Ident],
}

impl VisitMut for HostAssocRewriter<'_> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Type::Path(tp) = ty {
            let segs = &tp.path.segments;
            if tp.qself.is_none() && segs.len() == 2 && segs[0].ident == "Self" {
                let name = segs[1].ident.clone();
                if self.known.contains(&name) {
                    *ty = parse_quote!(#name);
                    return;
                }
            }
        }
        visit_mut::visit_type_mut(self, ty);
    }
}

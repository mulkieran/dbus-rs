
use std::{io, error, iter};
use dbus::arg::ArgType;
use xml;

fn find_attr<'a>(a: &'a Vec<xml::attribute::OwnedAttribute>, n: &str) -> Result<&'a str, Box<error::Error>> {
    a.into_iter().find(|q| q.name.local_name == n).map(|f| &*f.value).ok_or_else(|| "attribute not found".into())    
}

struct Arg {
    name: String,
    typ: String,
    idx: i32,
    is_out: bool,
}

struct Method {
    name: String,
    iargs: Vec<Arg>,
    oargs: Vec<Arg>,
}

struct Prop {
    name: String,
    typ: String,
    access: String,
}

struct Signal {
    _name: String,
    args: Vec<Arg>,
}

struct Intf {
    name: String,
    methods: Vec<Method>,
    props: Vec<Prop>,
    signals: Vec<Signal>,
}

fn make_camel(s: &str) -> String {
    let mut ucase = true;
    s.chars().filter_map(|c| match c {
        'a'...'z' | 'A'...'Z' | '0'...'9' => {
            let cc = if ucase { c.to_uppercase().next() } else { Some(c) };
            ucase = false;
            cc
        }
        _ => { ucase = true; None }
    }).collect()
}


fn make_snake(s: &str) -> String {
    let mut lcase = false;
    let mut r = String::new();
    for c in s.chars() {
        match c {
             'a'...'z' | '0'...'9' => {
                  r.push(c);
                  lcase = true;
             }
             'A'...'Z' => {
                  if lcase { r.push('_'); }
                  lcase = false;
                  r.push(c.to_lowercase().next().unwrap());
             }
             _ => { lcase = true; }
        }
    }
    r
}

fn xml_to_rust_type<I: Iterator<Item=char>>(i: &mut iter::Peekable<I>, out: bool) -> Result<String, Box<error::Error>> {
    let c = try!(i.next().ok_or_else(|| "unexpected end of signature"));
    let atype = ArgType::from_i32(c as i32);
    Ok(match atype {
        Ok(ArgType::Byte) => "u8".into(),
        Ok(ArgType::Boolean) => "bool".into(),
        Ok(ArgType::Int16) => "i16".into(),
        Ok(ArgType::UInt16) => "u16".into(),
        Ok(ArgType::Int32) => "i32".into(),
        Ok(ArgType::UInt32) => "u32".into(),
        Ok(ArgType::Int64) => "i64".into(),
        Ok(ArgType::UInt64) => "u64".into(),
        Ok(ArgType::Double) => "f64".into(),
        Ok(ArgType::String) => if out { "String".into() } else { "&str".into() },
        Ok(ArgType::ObjectPath) => if out { "::dbus::Path<'static>" } else { "::dbus::Path" }.into(),
        Ok(ArgType::Signature) => if out { "::dbus::Signature<'static>" } else { "::dbus::Signature" }.into(),
        Ok(ArgType::Variant) => "::dbus::arg::Variant<Box<::dbus::arg::RefArg>>".into(),
        Ok(ArgType::Array) => if i.peek() == Some(&'{') {
            i.next();
            let n1 = try!(xml_to_rust_type(i, out));
            let n2 = try!(xml_to_rust_type(i, out));
            if i.next() != Some('}') { return Err("No end of dict".into()); }
            format!("::std::collections::HashMap<{}, {}>", n1, n2)
        } else {
            format!("Vec<{}>", try!(xml_to_rust_type(i, out)))
        },
/*
 if out { format!("Vec<{}>", try!(xml_to_rust_type(i, out, false))) }
            else { format!("&[{}]", try!(xml_to_rust_type(i, out, false))) },
        Err(_) if c == '{' => {
            let n1 = try!(xml_to_rust_type(i, out, false));
            let n2 = try!(xml_to_rust_type(i, out, false));
            if i.next() != Some('}') { return Err("No end of dict".into()); }
            format!("({}, {})", n1, n2)
            },*/
        Err(_) if c == '(' => {
            let mut s: Vec<String> = vec!();
            while i.peek() != Some(&')') {
                let n = try!(xml_to_rust_type(i, out));
                s.push(n);
            };
            i.next().unwrap();
            format!("({})", s.join(", "))
        }
        // Err(_) if c == ')' && instruct => "".into(),
        a @ _ => panic!(format!("Unknown character in signature {:?}", a)),
    })
}

fn make_type(s: &str, out: bool) -> Result<String, Box<error::Error>> {
    let mut i = s.chars().peekable();
    let r = try!(xml_to_rust_type(&mut i, out));
    if i.next().is_some() { Err("Expected type to end".into()) }
    else { Ok(r) }
}

impl Arg {
    fn varname(&self) -> String {
        if self.name != "" {
           make_snake(&self.name)
        } else { format!("arg{}", self.idx) }
    }
    fn typename(&self) -> Result<String, Box<error::Error>> {
        make_type(&self.typ, self.is_out)
    }
}

impl Prop {
    fn can_get(&self) -> bool { self.access != "write" }
    fn can_set(&self) -> bool { self.access == "write" || self.access == "readwrite" }
}

fn write_method_decl(s: &mut String, m: &Method) -> Result<(), Box<error::Error>> {
    *s += &format!("    fn {}(&self", make_snake(&m.name));
    for a in m.iargs.iter() {
        let t = try!(a.typename());
        *s += &format!(", {}: {}", a.varname(), t);
    }
    match m.oargs.len() {
        0 => { *s += ") -> Result<(), ::dbus::Error>"; }
        1 => { *s += &format!(") -> Result<{}, ::dbus::Error>", try!(m.oargs[0].typename())); }
        _ => {
            *s += &format!(") -> Result<({}", try!(m.oargs[0].typename()));
            for z in m.oargs.iter().skip(1) { *s += &format!(", {}", try!(z.typename())); }
            *s += "), ::dbus::Error>";
        }
    }
    Ok(())
}

fn write_prop_decl(s: &mut String, p: &Prop, set: bool) -> Result<(), Box<error::Error>> {
    if set {
        *s += &format!("    fn set_{}(&self, value: {}) -> Result<(), ::dbus::Error>",
            make_snake(&p.name), try!(make_type(&p.typ, true)));
    } else {
        *s += &format!("    fn get_{}(&self) -> Result<{}, ::dbus::Error>",
            make_snake(&p.name), try!(make_type(&p.typ, true)));
    };
    Ok(())
}

fn write_intf(s: &mut String, i: &Intf) -> Result<(), Box<error::Error>> {
    
    let iname = make_camel(&i.name);  
    *s += &format!("\npub trait {} {{\n", iname);
    for m in &i.methods {
        try!(write_method_decl(s, &m));
        *s += ";\n";
    }
    for p in &i.props {
        if p.can_get() {
            try!(write_prop_decl(s, &p, false));
            *s += ";\n";
        }
        if p.can_set() {
            try!(write_prop_decl(s, &p, true));
            *s += ";\n";
        }
    }
    *s += "}\n";
    Ok(())
}

fn write_intf_client(s: &mut String, i: &Intf) -> Result<(), Box<error::Error>> {
    *s += &format!("\nimpl<'a, C: ::std::ops::Deref<Target=::dbus::Connection>> {} for ::dbus::ConnPath<'a, C> {{\n",
        make_camel(&i.name));
    for m in &i.methods {
        *s += "\n";
        try!(write_method_decl(s, &m));
        *s += " {\n";
        *s += &format!("        let mut m = try!(self.method_call_with_args(&\"{}\".into(), &\"{}\".into(), |{}| {{\n",
            i.name, m.name, if m.iargs.len() > 0 { "msg" } else { "_" } );
        if m.iargs.len() > 0 {
                *s += "            let mut i = ::dbus::arg::IterAppend::new(msg);\n";
        }
        for a in m.iargs.iter() {
                *s += &format!("            i.append({});\n", a.varname());
        }
        *s += "        }));\n";
        *s += "        try!(m.as_result());\n";
        if m.oargs.len() == 0 {
            *s += "        Ok(())\n";
        } else {
            *s += "        let mut i = m.iter_init();\n";
            for a in m.oargs.iter() {
                *s += &format!("        let {}: {} = try!(i.read());\n", a.varname(), try!(a.typename()));   
            }
            if m.oargs.len() == 1 {
                *s += &format!("        Ok({})\n", m.oargs[0].varname());
            } else {
                let v: Vec<String> = m.oargs.iter().map(|z| z.varname()).collect();
                *s += &format!("        Ok(({}))\n", v.join(", "));
            }
        }
        *s += "    }\n";
    }

    for p in i.props.iter().filter(|p| p.can_get()) {
        *s += "\n";
        try!(write_prop_decl(s, &p, false));
        *s += " {\n";
        *s += "        let mut m = try!(self.method_call_with_args(&\"Org.Freedesktop.DBus.Properties\".into(), &\"Get\".into(), move |msg| {\n";
        *s += "            let mut i = ::dbus::arg::IterAppend::new(msg);\n";
        *s += &format!("            i.append(\"{}\");\n", i.name);
        *s += &format!("            i.append(\"{}\");\n", p.name);
        *s += "        }));\n";
        *s += "        Ok(try!(try!(m.as_result()).read1()))\n";
        *s += "    }\n";
    }

    for p in i.props.iter().filter(|p| p.can_set()) {
        *s += "\n";
        try!(write_prop_decl(s, &p, true));
        *s += " {\n";
        *s += "        let mut m = try!(self.method_call_with_args(&\"Org.Freedesktop.DBus.Properties\".into(), &\"Set\".into(), move |msg| {\n";
        *s += "            let mut i = ::dbus::arg::IterAppend::new(msg);\n";
        *s += &format!("            i.append(\"{}\");\n", i.name);
        *s += &format!("            i.append(\"{}\");\n", p.name);
        *s += "            i.append(value);\n";
        *s += "        }));\n";
        *s += "        m.as_result()\n";
        *s += "    }\n";
    }

    *s += "}\n";
    Ok(())

}


// Should we implement this for
// 1) MethodInfo? That's the only way receiver can check Sender, etc.
// 2) D::ObjectPath?  
// 3) A user supplied struct?
// 4) Something reachable from minfo?

fn write_intf_tree(s: &mut String, i: &Intf, mtype: &str) -> Result<(), Box<error::Error>> {
    *s += &format!("\npub fn {}_server<F, T, D>(factory: &::dbus::tree::Factory<::dbus::tree::{}<D>, D>, data: D::Interface, f: F) -> ::dbus::tree::Interface<::dbus::tree::{}<D>, D>\n",
        make_snake(&i.name), mtype, mtype);
    *s += &format!("where D: ::dbus::tree::DataType, D::Method: Default, T: {}, \n", make_camel(&i.name));
    if i.props.len() > 0 {
        *s += "    D::Property: Default,";
    };
    *s += &format!("    F: 'static + for <'z> Fn(& 'z ::dbus::tree::MethodInfo<::dbus::tree::{}<D>, D>) -> & 'z T {{\n", mtype);
    *s += &format!("    let i = factory.interface(\"{}\", data);\n", i.name);
    *s += "    let f = ::std::sync::Arc::new(f);";
    for m in &i.methods {
        *s += "\n    let fclone = f.clone();\n";    
        *s += &format!("    let h = move |minfo: &::dbus::tree::MethodInfo<::dbus::tree::{}<D>, D>| {{\n", mtype);
        if m.iargs.len() > 0 {
            *s += "        let mut i = minfo.msg.iter_init();\n";
        }
        for a in &m.iargs {
            *s += &format!("        let {}: {} = try!(i.read());\n", a.varname(), try!(a.typename()));
        }
        *s += "        let d = fclone(minfo);\n";
        let argsvar = m.iargs.iter().map(|q| q.varname()).collect::<Vec<String>>().join(", ");
        let retargs = match m.oargs.len() {
            0 => String::new(),
            1 => format!("let {} = ", m.oargs[0].varname()),
            _ => format!("let ({}) = ", m.oargs.iter().map(|q| q.varname()).collect::<Vec<String>>().join(", ")),
        };
        *s += &format!("        {}try!(d.{}({}));\n",
            retargs, make_snake(&m.name), argsvar);
        *s += "        let rm = minfo.msg.method_return();\n";
        for r in &m.oargs {
            *s += &format!("        let rm = rm.append1({});\n", r.varname());
        }
        *s += "        Ok(vec!(rm))\n";
        *s += "    };\n";
        *s += &format!("    let m = factory.method(\"{}\", Default::default(), h);\n", m.name);
        for a in &m.iargs {
            *s += &format!("    let m = m.in_arg((\"{}\", \"{}\"));\n", a.name, a.typ);
        }
        for a in &m.oargs {
            *s += &format!("    let m = m.out_arg((\"{}\", \"{}\"));\n", a.name, a.typ);
        }
        *s +=          "    let i = i.add_m(m);\n";
    }
    for p in &i.props {
        *s += &format!("\n    let p = factory.property::<{}, _>(\"{}\", Default::default());\n", try!(make_type(&p.typ, false)), p.name);
        *s += &format!("    let p = p.access(::dbus::tree::Access::{});\n", match &*p.access {
            "read" => "Read",
            "readwrite" => "ReadWrite",
            "write" => "Write",
            _ => return Err(format!("Unexpected access value {}", p.access).into()),
        });
        if p.can_get() {
            *s += "    let fclone = f.clone();\n";    
            *s += "    let p = p.on_get(move |a, pinfo| {\n";
            *s += "        let minfo = pinfo.to_method_info();\n";
            *s += "        let d = fclone(&minfo);\n";
            *s += &format!("        a.append(try!(d.get_{}()));\n", make_snake(&p.name));
            *s += "        Ok(())\n";
            *s += "    });\n";
        }
        if p.can_set() {
            *s += "    let fclone = f.clone();\n";    
            *s += "    let p = p.on_set(move |iter, pinfo| {\n";
            *s += "        let minfo = pinfo.to_method_info();\n";
            *s += "        let d = fclone(&minfo);\n";
            *s += &format!("        try!(d.set_{}(try!(iter.read())));\n", make_snake(&p.name));
            *s += "        Ok(())\n";
            *s += "    });\n";
        }
        *s +=          "    let i = i.add_p(p);\n";
    }
    *s +=          "    i\n";
    *s +=          "}\n";
    Ok(())
}

pub fn generate(xmldata: &str, mtype: Option<&str>) -> Result<String, Box<error::Error>> {
    use xml::EventReader;
    use xml::reader::XmlEvent;

    let mut s = String::new();
    let mut curintf = None;
    let mut curm = None;
    let mut cursig = None;
    let mut curprop = None;
    let parser = EventReader::new(io::Cursor::new(xmldata));
    for e in parser {
        match try!(e) {
            XmlEvent::StartElement { ref name, ref attributes, .. } if &name.local_name == "interface" => {
                if curm.is_some() { try!(Err("Start of Interface inside method")) };
                if curintf.is_some() { try!(Err("Start of Interface inside interface")) };
                curintf = Some(Intf { name: try!(find_attr(attributes, "name")).into(), 
                    methods: Vec::new(), signals: Vec::new(), props: Vec::new() });
            }
            XmlEvent::EndElement { ref name } if &name.local_name == "interface" => {
                if curm.is_some() { try!(Err("End of Interface inside method")) };
                if curintf.is_none() { try!(Err("End of Interface outside interface")) };
                let intf = curintf.take().unwrap();
                try!(write_intf(&mut s, &intf));
                try!(write_intf_client(&mut s, &intf));
                if let Some(mt) = mtype {
                    try!(write_intf_tree(&mut s, &intf, mt));
                }
            }

            XmlEvent::StartElement { ref name, ref attributes, .. } if &name.local_name == "method" => {
                if curm.is_some() { try!(Err("Start of method inside method")) };
                if curintf.is_none() { try!(Err("Start of method outside interface")) };
                curm = Some(Method { name: try!(find_attr(attributes, "name")).into(), iargs: Vec::new(), oargs: Vec::new() });
            }
            XmlEvent::EndElement { ref name } if &name.local_name == "method" => {
                if curm.is_none() { try!(Err("End of method outside method")) };
                if curintf.is_none() { try!(Err("End of method outside interface")) };
                curintf.as_mut().unwrap().methods.push(curm.take().unwrap());
            }

            XmlEvent::StartElement { ref name, ref attributes, .. } if &name.local_name == "signal" => {
                if cursig.is_some() { try!(Err("Start of signal inside signal")) };
                if curintf.is_none() { try!(Err("Start of signal outside interface")) };
                cursig = Some(Signal { _name: try!(find_attr(attributes, "name")).into(), args: Vec::new() });
            }
            XmlEvent::EndElement { ref name } if &name.local_name == "signal" => {
                if cursig.is_none() { try!(Err("End of signal outside signal")) };
                if curintf.is_none() { try!(Err("End of signal outside interface")) };
                curintf.as_mut().unwrap().signals.push(cursig.take().unwrap());
            }

            XmlEvent::StartElement { ref name, ref attributes, .. } if &name.local_name == "property" => {
                if curprop.is_some() { try!(Err("Start of property inside property")) };
                if curintf.is_none() { try!(Err("Start of property outside interface")) };
                curprop = Some(Prop {
                    name: try!(find_attr(attributes, "name")).into(), 
                    typ: try!(find_attr(attributes, "type")).into(), 
                    access: try!(find_attr(attributes, "access")).into(), 
                });
            }
            XmlEvent::EndElement { ref name } if &name.local_name == "property" => {
                if curprop.is_none() { try!(Err("End of property outside property")) };
                if curintf.is_none() { try!(Err("End of property outside interface")) };
                curintf.as_mut().unwrap().props.push(curprop.take().unwrap());
            }


            XmlEvent::StartElement { ref name, ref attributes, .. } if &name.local_name == "arg" => {
                if curm.is_none() && cursig.is_none() { try!(Err("Start of arg outside method and signal")) };
                if curintf.is_none() { try!(Err("Start of arg outside interface")) };
                let typ = try!(find_attr(attributes, "type")).into();
                let is_out = if cursig.is_some() { true } else { match find_attr(attributes, "direction") {
                    Err(_) => false,
                    Ok("in") => false,
                    Ok("out") => true,
                    _ => { try!(Err("Invalid direction")); unreachable!() }
                }};
                let arr = if let Some(ref mut sig) = cursig { &mut sig.args }
                    else if is_out { &mut curm.as_mut().unwrap().oargs } else { &mut curm.as_mut().unwrap().iargs }; 
                let arg = Arg { name: find_attr(attributes, "name").unwrap_or("").into(),
                    typ: typ, is_out: is_out, idx: arr.len() as i32 };
                arr.push(arg);
            }
            _ => (),
        }
    }
    if curintf.is_some() { try!(Err("Unterminated interface")) }
    Ok(s)
}

#[cfg(test)]
mod tests {

use super::generate;

static FROM_DBUS: &'static str = r#"
<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="org.freedesktop.DBus">
    <method name="Hello">
      <arg direction="out" type="s"/>
    </method>
    <method name="RequestName">
      <arg direction="in" type="s"/>
      <arg direction="in" type="u"/>
      <arg direction="out" type="u"/>
    </method>
    <method name="ReleaseName">
      <arg direction="in" type="s"/>
      <arg direction="out" type="u"/>
    </method>
    <method name="StartServiceByName">
      <arg direction="in" type="s"/>
      <arg direction="in" type="u"/>
      <arg direction="out" type="u"/>
    </method>
    <method name="UpdateActivationEnvironment">
      <arg direction="in" type="a{ss}"/>
    </method>
    <method name="NameHasOwner">
      <arg direction="in" type="s"/>
      <arg direction="out" type="b"/>
    </method>
    <method name="ListNames">
      <arg direction="out" type="as"/>
    </method>
    <method name="ListActivatableNames">
      <arg direction="out" type="as"/>
    </method>
    <method name="AddMatch">
      <arg direction="in" type="s"/>
    </method>
    <method name="RemoveMatch">
      <arg direction="in" type="s"/>
    </method>
    <method name="GetNameOwner">
      <arg direction="in" type="s"/>
      <arg direction="out" type="s"/>
    </method>
    <method name="ListQueuedOwners">
      <arg direction="in" type="s"/>
      <arg direction="out" type="as"/>
    </method>
    <method name="GetConnectionUnixUser">
      <arg direction="in" type="s"/>
      <arg direction="out" type="u"/>
    </method>
    <method name="GetConnectionUnixProcessID">
      <arg direction="in" type="s"/>
      <arg direction="out" type="u"/>
    </method>
    <method name="GetAdtAuditSessionData">
      <arg direction="in" type="s"/>
      <arg direction="out" type="ay"/>
    </method>
    <method name="GetConnectionSELinuxSecurityContext">
      <arg direction="in" type="s"/>
      <arg direction="out" type="ay"/>
    </method>
    <method name="GetConnectionAppArmorSecurityContext">
      <arg direction="in" type="s"/>
      <arg direction="out" type="s"/>
    </method>
    <method name="ReloadConfig">
    </method>
    <method name="GetId">
      <arg direction="out" type="s"/>
    </method>
    <method name="GetConnectionCredentials">
      <arg direction="in" type="s"/>
      <arg direction="out" type="a{sv}"/>
    </method>
    <signal name="NameOwnerChanged">
      <arg type="s"/>
      <arg type="s"/>
      <arg type="s"/>
    </signal>
    <signal name="NameLost">
      <arg type="s"/>
    </signal>
    <signal name="NameAcquired">
      <arg type="s"/>
    </signal>
  </interface>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg direction="out" type="s"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Monitoring">
    <method name="BecomeMonitor">
      <arg direction="in" type="as"/>
      <arg direction="in" type="u"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Debug.Stats">
    <method name="GetStats">
      <arg direction="out" type="a{sv}"/>
    </method>
    <method name="GetConnectionStats">
      <arg direction="in" type="s"/>
      <arg direction="out" type="a{sv}"/>
    </method>
    <method name="GetAllMatchRules">
      <arg direction="out" type="a{sas}"/>
    </method>
  </interface>
</node>
"#; 

static FROM_POLICYKIT: &'static str = r#"
<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
                      "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<!-- GDBus 2.48.1 -->
<node>
  <interface name="org.freedesktop.DBus.Properties">
    <method name="Get">
      <arg type="s" name="interface_name" direction="in"/>
      <arg type="s" name="property_name" direction="in"/>
      <arg type="v" name="value" direction="out"/>
    </method>
    <method name="GetAll">
      <arg type="s" name="interface_name" direction="in"/>
      <arg type="a{sv}" name="properties" direction="out"/>
    </method>
    <method name="Set">
      <arg type="s" name="interface_name" direction="in"/>
      <arg type="s" name="property_name" direction="in"/>
      <arg type="v" name="value" direction="in"/>
    </method>
    <signal name="PropertiesChanged">
      <arg type="s" name="interface_name"/>
      <arg type="a{sv}" name="changed_properties"/>
      <arg type="as" name="invalidated_properties"/>
    </signal>
  </interface>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg type="s" name="xml_data" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Peer">
    <method name="Ping"/>
    <method name="GetMachineId">
      <arg type="s" name="machine_uuid" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.PolicyKit1.Authority">
    <method name="EnumerateActions">
      <arg type="s" name="locale" direction="in">
      </arg>
      <arg type="a(ssssssuuua{ss})" name="action_descriptions" direction="out">
      </arg>
    </method>
    <method name="CheckAuthorization">
      <arg type="(sa{sv})" name="subject" direction="in">
      </arg>
      <arg type="s" name="action_id" direction="in">
      </arg>
      <arg type="a{ss}" name="details" direction="in">
      </arg>
      <arg type="u" name="flags" direction="in">
      </arg>
      <arg type="s" name="cancellation_id" direction="in">
      </arg>
      <arg type="(bba{ss})" name="result" direction="out">
      </arg>
    </method>
    <method name="CancelCheckAuthorization">
      <arg type="s" name="cancellation_id" direction="in">
      </arg>
    </method>
    <method name="RegisterAuthenticationAgent">
      <arg type="(sa{sv})" name="subject" direction="in">
      </arg>
      <arg type="s" name="locale" direction="in">
      </arg>
      <arg type="s" name="object_path" direction="in">
      </arg>
    </method>
    <method name="RegisterAuthenticationAgentWithOptions">
      <arg type="(sa{sv})" name="subject" direction="in">
      </arg>
      <arg type="s" name="locale" direction="in">
      </arg>
      <arg type="s" name="object_path" direction="in">
      </arg>
      <arg type="a{sv}" name="options" direction="in">
      </arg>
    </method>
    <method name="UnregisterAuthenticationAgent">
      <arg type="(sa{sv})" name="subject" direction="in">
      </arg>
      <arg type="s" name="object_path" direction="in">
      </arg>
    </method>
    <method name="AuthenticationAgentResponse">
      <arg type="s" name="cookie" direction="in">
      </arg>
      <arg type="(sa{sv})" name="identity" direction="in">
      </arg>
    </method>
    <method name="AuthenticationAgentResponse2">
      <arg type="u" name="uid" direction="in">
      </arg>
      <arg type="s" name="cookie" direction="in">
      </arg>
      <arg type="(sa{sv})" name="identity" direction="in">
      </arg>
    </method>
    <method name="EnumerateTemporaryAuthorizations">
      <arg type="(sa{sv})" name="subject" direction="in">
      </arg>
      <arg type="a(ss(sa{sv})tt)" name="temporary_authorizations" direction="out">
      </arg>
    </method>
    <method name="RevokeTemporaryAuthorizations">
      <arg type="(sa{sv})" name="subject" direction="in">
      </arg>
    </method>
    <method name="RevokeTemporaryAuthorizationById">
      <arg type="s" name="id" direction="in">
      </arg>
    </method>
    <signal name="Changed">
    </signal>
    <property type="s" name="BackendName" access="read">
    </property>
    <property type="s" name="BackendVersion" access="read">
    </property>
    <property type="u" name="BackendFeatures" access="read">
    </property>
  </interface>
</node>
"#; 

    #[test]
    fn from_dbus() {
        let s = generate(FROM_DBUS, Some("MTSync")).unwrap();
        println!("{}", s);
        //assert_eq!(s, "fdjsf");
    }

    #[test]
    fn from_policykit() {
        let s = generate(FROM_POLICYKIT, Some("MTFn")).unwrap();
        println!("{}", s);
        let mut f = ::std::fs::File::create("./tests/generated/mod.rs").unwrap();
        (&mut f as &mut ::std::io::Write).write_all(s.as_bytes()).unwrap();
        drop(f);
        // assert_eq!(s, "fdjsf");
    }

}

use crate::avm1::function::FunctionObject;
use crate::avm1::globals::shared_object::{deserialize_value, serialize};
use crate::avm1::object::Object;
use crate::avm1::property_decl::{define_properties_on, Declaration};
use crate::avm1::{Activation, ActivationIdentifier, Error, ExecutionReason, NativeObject, Value};
use crate::avm1_stub;
use crate::context::UpdateContext;
use crate::net_connection::{NetConnectionHandle, NetConnections, ResponderCallback};
use crate::string::{AvmString, StringContext};
use flash_lso::packet::Header;
use flash_lso::types::ObjectId;
use flash_lso::types::Value as AMFValue;
use gc_arena::{Collect, Gc};
use ruffle_macros::istr;
use ruffle_wstr::WStr;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Clone, Debug, Collect)]
#[collect(require_static)]
struct NetConnectionData {
    handle: Cell<Option<NetConnectionHandle>>,
}

#[derive(Copy, Clone, Debug, Collect)]
#[collect(no_drop)]
pub struct NetConnection<'gc>(Gc<'gc, NetConnectionData>);

impl<'gc> NetConnection<'gc> {
    pub fn handle(&self) -> Option<NetConnectionHandle> {
        self.0.handle.get()
    }

    pub fn set_handle(&self, handle: Option<NetConnectionHandle>) -> Option<NetConnectionHandle> {
        self.0.handle.replace(handle)
    }

    pub fn cast(value: Value<'gc>) -> Option<Self> {
        if let Value::Object(object) = value {
            if let NativeObject::NetConnection(net_connection) = object.native() {
                return Some(net_connection);
            }
        }
        None
    }

    pub fn on_status_event(
        context: &mut UpdateContext<'gc>,
        this: Object<'gc>,
        code: &'static str,
    ) -> Result<(), Error<'gc>> {
        let Some(root_clip) = context.stage.root_clip() else {
            tracing::warn!("Ignored NetConnection callback as there's no root movie");
            return Ok(());
        };
        let mut activation = Activation::from_nothing(
            context,
            ActivationIdentifier::root("[NetConnection connect]"),
            root_clip,
        );
        let constructor = activation.context.avm1.prototypes().object_constructor;
        let event = constructor
            .construct(&mut activation, &[])?
            .coerce_to_object(&mut activation);
        let code = AvmString::new_utf8(activation.gc(), code);
        event.set(istr!("code"), code.into(), &mut activation)?;
        event.set(istr!("level"), istr!("status").into(), &mut activation)?;
        this.call_method(
            istr!("onStatus"),
            &[event.into()],
            &mut activation,
            ExecutionReason::Special,
        )?;
        Ok(())
    }

    // [NA] I have no idea why this is a thing. It's similar in AVM2 too.
    pub fn on_empty_status_event(
        context: &mut UpdateContext<'gc>,
        this: Object<'gc>,
    ) -> Result<(), Error<'gc>> {
        let Some(root_clip) = context.stage.root_clip() else {
            tracing::warn!("Ignored NetConnection callback as there's no root movie");
            return Ok(());
        };
        let mut activation = Activation::from_nothing(
            context,
            ActivationIdentifier::root("[NetConnection connect]"),
            root_clip,
        );
        this.call_method(
            istr!("onStatus"),
            &[],
            &mut activation,
            ExecutionReason::Special,
        )?;
        Ok(())
    }

    pub fn send_callback(
        context: &mut UpdateContext<'gc>,
        responder: Object<'gc>,
        callback: ResponderCallback,
        message: &flash_lso::types::Value,
    ) -> Result<(), Error<'gc>> {
        let Some(root_clip) = context.stage.root_clip() else {
            tracing::warn!("Ignored NetConnection response as there's no root movie");
            return Ok(());
        };
        let mut activation = Activation::from_nothing(
            context,
            ActivationIdentifier::root("[NetConnection response]"),
            root_clip,
        );
        let method_name = match callback {
            ResponderCallback::Result => istr!("onResult"),
            ResponderCallback::Status => istr!("onStatus"),
        };
        let reader = flash_lso::read::Reader::default();
        let mut reference_cache = BTreeMap::default();
        let value = deserialize_value(
            &mut activation,
            message,
            &reader.amf0_decoder,
            &mut reference_cache,
        );
        responder.call_method(
            method_name,
            &[value],
            &mut activation,
            ExecutionReason::Special,
        )?;
        Ok(())
    }
}

pub fn constructor<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let net_connection = NetConnection(Gc::new(
        activation.gc(),
        NetConnectionData {
            handle: Cell::new(None),
        },
    ));

    this.set_native(activation.gc(), NativeObject::NetConnection(net_connection));
    Ok(Value::Undefined)
}

const PROTO_DECLS: &[Declaration] = declare_properties! {
    "isConnected" => property(is_connected);
    "protocol" => property(protocol);
    "uri" => property(uri);

    "addHeader" => method(add_header; DONT_ENUM | DONT_DELETE);
    "call" => method(call; DONT_ENUM | DONT_DELETE);
    "close" => method(close; DONT_ENUM | DONT_DELETE);
    "connect" => method(connect; DONT_ENUM | DONT_DELETE);
    "getService" => method(get_service; DONT_ENUM | DONT_DELETE);
    "realcall" => method(realcall; DONT_ENUM | DONT_DELETE);
};

fn is_connected<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(net_connection) = NetConnection::cast(this.into()) {
        return Ok(net_connection
            .handle()
            .map(|handle| activation.context.net_connections.is_connected(handle))
            .unwrap_or_default()
            .into());
    }
    Ok(Value::Undefined)
}

fn protocol<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(net_connection) = NetConnection::cast(this.into()) {
        return if let Some(protocol) = net_connection
            .handle()
            .and_then(|handle| activation.context.net_connections.get_protocol(handle))
        {
            Ok(AvmString::new_utf8(activation.gc(), protocol).into())
        } else {
            Ok(Value::Undefined)
        };
    }
    Ok(Value::Undefined)
}

fn uri<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(net_connection) = NetConnection::cast(this.into()) {
        return if let Some(uri) = net_connection
            .handle()
            .and_then(|handle| activation.context.net_connections.get_uri(handle))
        {
            Ok(Value::String(AvmString::new_utf8(activation.gc(), uri)))
        } else {
            Ok(Value::Undefined)
        };
    }
    Ok(Value::Undefined)
}

fn add_header<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let Some(net_connection) = NetConnection::cast(this.into()) else {
        return Ok(Value::Undefined);
    };

    let name = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .coerce_to_string(activation)?;
    let must_understand = args
        .get(1)
        .unwrap_or(&Value::Bool(true))
        .as_bool(activation.swf_version());

    let value = serialize(activation, *args.get(2).unwrap_or(&Value::Null));

    if let Some(handle) = net_connection.handle() {
        activation.context.net_connections.set_header(
            handle,
            Header {
                name: name.to_string(),
                must_understand,
                value: Rc::new(value),
            },
        );
    }

    Ok(Value::Undefined)
}

fn call<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // Log ALL NetConnection.call() invocations to trace GUI behavior
    if let Some(method_name) = args.get(0) {
        let method_str = method_name.coerce_to_string(activation)
            .unwrap_or_else(|_| AvmString::new_utf8(activation.gc(), "<error>"));

        // Use AVM trace to ensure it shows up in browser console
        let trace_msg = format!("🔥 NetConnection.call() (REAL) invoked with method: '{}' and {} total args", method_str, args.len());
        activation.context.avm_trace(&trace_msg);

        // Log all arguments for debugging
        for (i, arg) in args.iter().enumerate() {
            let arg_str = format!("{:?}", arg);
            let preview = if arg_str.len() > 100 {
                format!("{}...", &arg_str[..100])
            } else {
                arg_str
            };
            let arg_trace = format!("  Arg[{}]: {}", i, preview);
            activation.context.avm_trace(&arg_trace);
        }
    }
    let Some(net_connection) = NetConnection::cast(this.into()) else {
        return Ok(Value::Undefined);
    };

    let command = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .coerce_to_string(activation)?;
    let mut arguments = Vec::new();

    for arg in &args[2..] {
        arguments.push(Rc::new(serialize(activation, *arg)));
    }

    if let Some(handle) = net_connection.handle() {
        if let Some(responder) = args.get(1) {
            let responder = responder.coerce_to_object(activation);
            NetConnections::send_avm1(
                activation.context,
                handle,
                command.to_string(),
                AMFValue::StrictArray(ObjectId::INVALID, arguments),
                responder,
            );
        } else {
            NetConnections::send_without_response(
                activation.context,
                handle,
                command.to_string(),
                AMFValue::StrictArray(ObjectId::INVALID, arguments),
            );
        }
    }

    Ok(Value::Undefined)
}

fn close<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(net_connection) = NetConnection::cast(this.into()) {
        if let Some(previous_handle) = net_connection.set_handle(None) {
            NetConnections::close(activation.context, previous_handle, true);
        }
    }
    Ok(Value::Undefined)
}

/// The realcall method - this is what the debug proxy calls
fn realcall<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // Log realcall invocations - this is what the SWF debug proxy actually calls
    if let Some(method_name) = args.get(0) {
        let method_str = method_name.coerce_to_string(activation)
            .unwrap_or_else(|_| AvmString::new_utf8(activation.gc(), "<error>"));

        let trace_msg = format!("🚀 NetConnection.realcall() (DEBUG PROXY) invoked with method: '{}' and {} total args", method_str, args.len());
        activation.context.avm_trace(&trace_msg);

        // Log all arguments for debugging
        for (i, arg) in args.iter().enumerate() {
            let arg_str = format!("{:?}", arg);
            let preview = if arg_str.len() > 100 {
                format!("{}...", &arg_str[..100])
            } else {
                arg_str
            };
            let arg_trace = format!("  RealCall Arg[{}]: {}", i, preview);
            activation.context.avm_trace(&arg_trace);
        }
    }

    // Call the original call method
    call(activation, this, args)
}

fn connect<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // Log NetConnection.connect() calls
    if let Some(url_arg) = args.get(0) {
        let url_str = url_arg.coerce_to_string(activation)
            .unwrap_or_else(|_| AvmString::new_utf8(activation.gc(), "<error>"));
        let trace_msg = format!("🌐 NetConnection.connect() called with URL: '{}'", url_str);
        activation.context.avm_trace(&trace_msg);
    }
    if matches!(
        args.get(0),
        None | Some(Value::Undefined) | Some(Value::Null)
    ) {
        NetConnections::connect_to_local(activation.context, this);
        return Ok(Value::Undefined);
    }

    let url = args[0].coerce_to_string(activation)?;
    if url.starts_with(WStr::from_units(b"http://"))
        || url.starts_with(WStr::from_units(b"https://"))
    {
        // HTTP(S) is for Flash Remoting, which is just POST requests to the URL.
        NetConnections::connect_to_flash_remoting(activation.context, this, url.to_string());
    } else {
        avm1_stub!(
            activation,
            "NetConnection",
            "connect",
            "with non-null, non-http command"
        );
    }

    Ok(Value::Undefined)
}

fn get_service<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    activation.context.avm_trace("🎯 getService() called - our implementation is working!");
    let Some(net_connection) = NetConnection::cast(this.into()) else {
        return Ok(Value::Undefined);
    };

    // Check if NetConnection is connected
    if !net_connection
        .handle()
        .map(|handle| activation.context.net_connections.is_connected(handle))
        .unwrap_or_default()
    {
        return Ok(Value::Undefined);
    }

    let service_name = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .coerce_to_string(activation)?;

    let client = args.get(1).cloned().unwrap_or(Value::Undefined);

    // Create NetServiceProxy object
    let net_service_proxy = create_net_service_proxy(activation, this, service_name, client)?;

    Ok(net_service_proxy.into())
}

/// Creates a NetServiceProxy object that acts as a proxy for Flash Remoting service calls
fn create_net_service_proxy<'gc>(
    activation: &mut Activation<'_, 'gc>,
    net_connection: Object<'gc>,
    service_name: AvmString<'gc>,
    client: Value<'gc>,
) -> Result<Object<'gc>, Error<'gc>> {
    tracing::info!("Creating NetServiceProxy for service: {}", service_name);

    let constructor = activation.context.avm1.prototypes().object_constructor;
    let proxy = constructor
        .construct(activation, &[])?
        .coerce_to_object(activation);

    // Set properties on the NetServiceProxy
    proxy.set(AvmString::new_utf8(activation.gc(), "nc"), net_connection.into(), activation)?;
    proxy.set(AvmString::new_utf8(activation.gc(), "serviceName"), service_name.into(), activation)?;
    proxy.set(AvmString::new_utf8(activation.gc(), "client"), client, activation)?;

    // Set the __resolve method that will intercept method calls
    let resolve_fn = FunctionObject::native(
        &activation.context.strings,
        net_service_proxy_resolve,
        activation.context.avm1.prototypes().function,
        activation.context.avm1.prototypes().object,
    );
    proxy.set(AvmString::new_utf8(activation.gc(), "__resolve"), resolve_fn.into(), activation)?;

    tracing::info!("NetServiceProxy created with __resolve method set");

    Ok(proxy)
}

/// The __resolve method for NetServiceProxy that intercepts method calls
fn net_service_proxy_resolve<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let method_name = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .coerce_to_string(activation)?;

    // Debug logging
    tracing::info!("NetServiceProxy __resolve called for method: {}", method_name);

    // Store the method name on the proxy object for later use
    this.set(
        AvmString::new_utf8(activation.gc(), "__currentMethod"),
        method_name.into(),
        activation,
    )?;

    // Create a function that will make the actual remote call
    let remote_call_fn = FunctionObject::native(
        &activation.context.strings,
        net_service_proxy_call,
        activation.context.avm1.prototypes().function,
        activation.context.avm1.prototypes().object,
    );

    tracing::info!("NetServiceProxy __resolve returning function for method: {}", method_name);
    Ok(remote_call_fn.into())
}

/// Makes the actual Flash Remoting call through NetConnection.call()
fn net_service_proxy_call<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    tracing::info!("NetServiceProxy call function invoked with {} args", args.len());

    // Get the proxy object from the function's context
    // In AVM1, 'this' in the function call refers to the proxy object
    let proxy = this;

    // Get the stored information from the proxy object
    let net_connection = proxy.get(AvmString::new_utf8(activation.gc(), "nc"), activation)?.coerce_to_object(activation);
    let service_name = proxy.get(AvmString::new_utf8(activation.gc(), "serviceName"), activation)?.coerce_to_string(activation)?;
    let method_name = proxy.get(
        AvmString::new_utf8(activation.gc(), "__currentMethod"),
        activation,
    )?.coerce_to_string(activation)?;
    let client = proxy.get(AvmString::new_utf8(activation.gc(), "client"), activation)?;

    tracing::info!("NetServiceProxy calling {}.{} with {} args", service_name, method_name, args.len());

    // Build the full method name: serviceName.methodName
    let full_method_name = AvmString::concat(
        activation.gc(),
        service_name,
        AvmString::concat(
            activation.gc(),
            AvmString::new_utf8(activation.gc(), "."),
            method_name,
        ),
    );

    // For now, create a simple responder that will call back to the client
    let responder = if !matches!(client, Value::Undefined | Value::Null) {
        let constructor = activation.context.avm1.prototypes().object_constructor;
        let responder = constructor
            .construct(activation, &[])?
            .coerce_to_object(activation);

        // Store client and method name for callback
        responder.set(AvmString::new_utf8(activation.gc(), "client"), client, activation)?;
        responder.set(
            AvmString::new_utf8(activation.gc(), "methodName"),
            method_name.into(),
            activation,
        )?;

        // Set onResult method
        let on_result_fn = FunctionObject::native(
            &activation.context.strings,
            net_service_proxy_on_result,
            activation.context.avm1.prototypes().function,
            activation.context.avm1.prototypes().object,
        );
        responder.set(AvmString::new_utf8(activation.gc(), "onResult"), on_result_fn.into(), activation)?;

        // Set onStatus method for error handling
        let on_status_fn = FunctionObject::native(
            &activation.context.strings,
            net_service_proxy_on_status,
            activation.context.avm1.prototypes().function,
            activation.context.avm1.prototypes().object,
        );
        responder.set(AvmString::new_utf8(activation.gc(), "onStatus"), on_status_fn.into(), activation)?;

        Some(responder.into())
    } else {
        None
    };

    // Prepare arguments for NetConnection.call()
    let mut call_args = vec![full_method_name.into()];
    if let Some(responder) = responder {
        call_args.push(responder);
    } else {
        call_args.push(Value::Null);
    }
    call_args.extend_from_slice(args);

    // Call NetConnection.call()
    tracing::info!("NetServiceProxy about to call NetConnection.call() with method: {}", full_method_name);
    let result = net_connection.call_method(
        AvmString::new_utf8(activation.gc(), "call"),
        &call_args,
        activation,
        ExecutionReason::FunctionCall,
    );
    tracing::info!("NetServiceProxy NetConnection.call() completed with result: {:?}", result);
    result
}

/// Handles onResult callback for NetServiceProxy
fn net_service_proxy_on_result<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let client = this.get(AvmString::new_utf8(activation.gc(), "client"), activation)?;
    let method_name = this.get(
        AvmString::new_utf8(activation.gc(), "methodName"),
        activation,
    )?.coerce_to_string(activation)?;

    if !matches!(client, Value::Undefined | Value::Null) {
        let client_obj = client.coerce_to_object(activation);
        let result_method_name = AvmString::concat(
            activation.gc(),
            method_name,
            AvmString::new_utf8(activation.gc(), "_Result"),
        );

        client_obj.call_method(
            result_method_name,
            args,
            activation,
            ExecutionReason::FunctionCall,
        )?;
    }

    Ok(Value::Undefined)
}

/// Handles onStatus callback for NetServiceProxy
fn net_service_proxy_on_status<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let client = this.get(AvmString::new_utf8(activation.gc(), "client"), activation)?;
    let method_name = this.get(
        AvmString::new_utf8(activation.gc(), "methodName"),
        activation,
    )?.coerce_to_string(activation)?;

    if !matches!(client, Value::Undefined | Value::Null) {
        let client_obj = client.coerce_to_object(activation);
        let status_method_name = AvmString::concat(
            activation.gc(),
            method_name,
            AvmString::new_utf8(activation.gc(), "_Status"),
        );

        client_obj.call_method(
            status_method_name,
            args,
            activation,
            ExecutionReason::FunctionCall,
        )?;
    }

    Ok(Value::Undefined)
}

pub fn create_proto<'gc>(
    context: &mut StringContext<'gc>,
    proto: Object<'gc>,
    fn_proto: Object<'gc>,
) -> Object<'gc> {
    let object = Object::new(context, Some(proto));
    define_properties_on(PROTO_DECLS, context, object, fn_proto);
    object
}

pub fn create_class<'gc>(
    context: &mut StringContext<'gc>,
    netconnection_proto: Object<'gc>,
    fn_proto: Object<'gc>,
) -> Object<'gc> {
    FunctionObject::native(context, constructor, fn_proto, netconnection_proto)
}

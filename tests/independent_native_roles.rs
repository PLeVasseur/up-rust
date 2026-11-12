//! Native declaration/type fixtures only; FixtureCore is not installed middleware.
#[allow(dead_code)]
#[path = "../examples/support/independent_native.rs"]
mod apps;
#[path = "support/native_fixture.rs"]
mod native_fixture;
#[allow(dead_code)]
#[path = "../examples/support/independent_schema.rs"]
mod schema;

up_rust::native_roles! { mod empty { core: crate::native_fixture::FixtureCore; } }
up_rust::native_roles! {
    mod publisher_only {
        core: crate::native_fixture::FixtureCore;
        publisher ReadingPublisher, create: super::schema::TEMPERATURE => super::schema::Reading;
    }
}
up_rust::native_roles! {
    mod subscriber_only {
        core: crate::native_fixture::FixtureCore;
        subscription ReadingSubscription, create: super::schema::TEMPERATURE => super::schema::Reading;
    }
}
up_rust::native_roles! {
    mod client_only {
        core: crate::native_fixture::FixtureCore;
        client Client, create, Response: super::schema::READ => super::schema::Query, super::schema::Reading;
    }
}
up_rust::native_roles! {
    mod server_only {
        core: crate::native_fixture::FixtureCore;
        server Server, create: super::schema::READ => super::schema::Query, super::schema::Reading;
        reply u32 => |value, loan| loan.into_initializer().value(value).finish();
    }
}
up_rust::native_roles! {
    mod mixed {
        core: crate::native_fixture::FixtureCore;
        publisher First, first: super::schema::TEMPERATURE => super::schema::Reading;
        notifier Alarm, alarm: super::schema::ALARM => super::schema::Reading;
        publisher Second, second: super::schema::PRESSURE => super::schema::Reading;
        client Client, client: super::schema::READ => super::schema::Query, super::schema::Reading;
        client Backup, backup: super::schema::READ_BACKUP => super::schema::Query, super::schema::Reading;
        receiver Notices, notices: super::schema::ALERT => super::schema::Reading;
    }
}

#[test]
fn concrete_stored_roles_need_no_application_engine_aliases() {
    fn send<T: Send>() {}
    send::<apps::PublisherOnly>();
    send::<apps::SubscriberOnly>();
    send::<apps::ClientOnly>();
    send::<apps::ServerOnly>();
    send::<apps::MultiplePublishers>();
    send::<client_only::Response>();
}

#[allow(dead_code)]
fn two_publishers_one_local_deployment(
    local: &mixed::Deployment,
) -> Result<(mixed::First, mixed::Second), up_rust::SetupError> {
    Ok((mixed::first(local)?, mixed::second(local)?))
}

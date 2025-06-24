use fig_util::system_info::in_cloudshell;

const GOV_REGIONS: &[&str] = &["us-gov-east-1", "us-gov-west-1"];

pub fn region_check(capability: &'static str) -> eyre::Result<()> {
    let Ok(region) = std::env::var("AWS_REGION") else {
        return Ok(());
    };

    if in_cloudshell() && GOV_REGIONS.contains(&region.as_str()) {
        eyre::bail!("AWS GovCloud ({region}) is not supported for {capability}.");
    }

    Ok(())
}

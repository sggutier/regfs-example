use anyhow::Result;
use prjfs::provider::{Provider, ProviderT};
use prjfs::{NotificationType, OptionBuilder};

mod dirinfo;
mod regfs;
mod regop;

use crate::regfs::RegFs;

fn main() -> Result<()> {
    env_logger::init();
    let options = OptionBuilder::new().add_root_notification(
        NotificationType::FILE_OPENED | NotificationType::PRE_RENAME | NotificationType::PRE_DELETE,
    );
    let regfs: Box<dyn ProviderT> = Box::new(RegFs::new());

    let _provider = Provider::new("./test".into(), options, regfs)?;

    loop {}
}

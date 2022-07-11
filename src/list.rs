use color_eyre::{eyre::WrapErr, Result};

use crate::{
    api::rest::{Gateway, TableTask, TaskTree},
    close, edit,
};
use strum::{Display, EnumVariantNames, FromRepr, VariantNames};

#[derive(clap::Parser, Debug)]
pub struct Params {
    /// Specify a filter query to run against the Todoist API.
    #[clap(short='f', long="filter", default_value_t=String::from("(today | overdue)"))]
    filter: String,
    /// Run the list display in interactive mode to perform various actions on the items.
    #[clap(short = 'i')]
    interactive: bool,
}

/// List lists the tasks of the current user accessing the gateway with the given filter.
pub async fn list(params: Params, gw: &Gateway) -> Result<()> {
    let tasks = gw.tasks(Some(&params.filter)).await?;
    let tree = TaskTree::from_tasks(tasks).wrap_err("tasks do not form clean tree")?;
    // TODO: make from_tasks sort, too
    if params.interactive {
        match get_interactive_tasks(&tree)? {
            Some(task) => select_task_option(task, gw).await?,
            None => println!("No selection was made"),
        }
    } else {
        list_tasks(&tree);
    }
    Ok(())
}

pub fn get_interactive_tasks(tree: &[TaskTree]) -> Result<Option<&TaskTree>> {
    let result = dialoguer::FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .items(&tree.iter().map(|t| TableTask(&t.task)).collect::<Vec<_>>())
        .with_prompt("Select task")
        .default(0)
        .interact_opt()
        .wrap_err("Unable to make a selection")?;
    Ok(result.map(|index| &tree[index]))
}

fn list_tasks(tree: &[TaskTree]) {
    for task in tree.iter() {
        println!("{}", TableTask(&task.task));
    }
}

#[derive(Display, FromRepr, EnumVariantNames)]
enum TaskOptions {
    Close,
    Edit,
    Quit,
}

async fn select_task_option(task: &TaskTree, gw: &Gateway) -> Result<()> {
    println!("{}", task.task);
    let result = match make_selection(TaskOptions::VARIANTS)? {
        Some(index) => TaskOptions::from_repr(index).unwrap(),
        None => {
            println!("No selection made");
            return Ok(());
        }
    };
    match result {
        TaskOptions::Close => close::close(close::Params { id: task.task.id }, gw).await?,
        TaskOptions::Edit => edit_task(task, gw).await?,
        TaskOptions::Quit => {}
    };
    Ok(())
}

#[derive(Display, FromRepr, EnumVariantNames)]
enum EditOptions {
    Name,
    Description,
    Due,
    Priority,
    Quit,
}

async fn edit_task(task: &TaskTree, gw: &Gateway) -> Result<()> {
    // edit::edit(edit::Params { id: task.task.id }, gw).await?,
    let result = match make_selection(EditOptions::VARIANTS)? {
        Some(index) => EditOptions::from_repr(index).unwrap(),
        None => {
            println!("No selection made");
            return Ok(());
        }
    };
    match result {
        EditOptions::Quit => {}
        EditOptions::Priority => {
            let selection = dialoguer::Select::new()
                .with_prompt("Set priority")
                .items(&["1 - Normal", "2 - High", "3 - Very High", "4 - Urgent"])
                .default((task.task.priority as u8 - 1) as usize)
                .interact()
                .wrap_err("Bad user input")?
                + 1;
            let mut params = edit::Params::new(task.task.id);
            params.priority = Some(selection.try_into()?);
            edit::edit(params, gw).await?;
        }
        _ => {
            let text = dialoguer::Input::new()
                .with_prompt("New value")
                .interact_text()
                .wrap_err("Bad user input")?;
            let mut params = edit::Params::new(task.task.id);
            match result {
                EditOptions::Name => {
                    params.name = Some(text);
                }
                EditOptions::Description => {
                    params.desc = Some(text);
                }
                EditOptions::Due => {
                    params.due = Some(text);
                }
                EditOptions::Priority => unreachable!(),
                EditOptions::Quit => unreachable!(),
            };
            edit::edit(params, gw).await?;
        }
    };
    Ok(())
}

fn make_selection<T: ToString + std::fmt::Display>(variants: &[T]) -> Result<Option<usize>> {
    dialoguer::FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .items(variants)
        .default(0)
        .interact_opt()
        .wrap_err("Unable to make a selection")
}

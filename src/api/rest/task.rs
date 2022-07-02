use core::fmt;
use std::{
    cell::RefCell,
    collections::{
        hash_map::{Entry, HashMap},
        VecDeque,
    },
    fmt::Display,
    rc::Rc,
};

use color_eyre::eyre::bail;
use owo_colors::OwoColorize;
use serde::{de::Deserializer, Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

pub type TaskID = u64;
pub type ProjectID = u64;
pub type SectionID = u64;
pub type LabelID = u64;
pub type UserID = u64;

/// Priority as is given from the todoist API.
///
/// 1 for Normal up to 4 for Urgent.
#[derive(Debug, Serialize_repr, Deserialize_repr)]
#[repr(i8)]
pub enum Priority {
    Normal = 1,
    High = 2,
    VeryHigh = 3,
    Urgent = 4,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

fn deserialize_zero_to_none<'de, D, T: Deserialize<'de> + num_traits::Zero>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Value<U>(Option<U>);
    let v: Value<T> = Deserialize::deserialize(deserializer)?;
    let result = match v.0 {
        Some(v) => {
            if v.is_zero() {
                None
            } else {
                Some(v)
            }
        }
        None => None,
    };
    Ok(result)
}

/// Task describes a Task from the todoist API.
///
/// Taken from https://developer.todoist.com/rest/v1/#tasks.
#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskID,
    pub project_id: ProjectID,
    #[serde(deserialize_with = "deserialize_zero_to_none")]
    pub section_id: Option<SectionID>, // TODO: can be 0 -> map to None?
    pub content: String,
    pub description: String,
    pub completed: bool,
    pub label_ids: Vec<LabelID>,
    pub parent_id: Option<TaskID>,
    pub order: isize,
    pub priority: Priority,
    pub due: Option<DueDate>,
    pub url: String,
    pub comment_count: usize,
    pub assignee: Option<UserID>,
    #[serde(deserialize_with = "deserialize_zero_to_none")]
    pub assigner: Option<UserID>, // TODO: can be 0 -> map to None?
    pub created: chrono::DateTime<chrono::Utc>,
}

pub struct TableTask<'a>(pub &'a Task);

impl Display for TableTask<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}",
            self.0.id.bright_red(),
            self.0.content.default_color()
        )
    }
}

/// ExactTime exists in DueDate if this is an exact DueDate.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExactTime {
    pub datetime: chrono::DateTime<chrono::FixedOffset>,
    pub timezone: String,
}

/// DueDate is the Due object from the todoist API.
///
/// Mostly contains human-readable content for easier display.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DueDate {
    #[serde(alias = "string")]
    pub human_readable: String,
    pub date: String,
    pub recurring: bool,
    #[serde(flatten)]
    pub exact: Option<ExactTime>,
}

#[derive(Debug)]
pub struct TaskTree {
    pub task: Task,
    pub subtasks: Vec<TaskTree>,
}

#[derive(Debug)]
struct TaskTreeBuilder {
    task: Task,
    parent: Option<()>,
    subtasks: Vec<Rc<RefCell<TaskTreeBuilder>>>,
}

impl TaskTreeBuilder {
    fn finalize(self) -> TaskTree {
        let subtasks: Vec<TaskTree> = self
            .subtasks
            .into_iter()
            .map(|c| {
                Rc::try_unwrap(c)
                    .expect("should consume single Rc")
                    .into_inner()
                    .finalize()
            })
            .collect();
        TaskTree {
            task: self.task,
            subtasks,
        }
    }
}

impl TaskTree {
    pub fn from_tasks(tasks: Vec<Task>) -> color_eyre::Result<Vec<TaskTree>> {
        let (top_level_tasks, mut subtasks): (VecDeque<_>, VecDeque<_>) = tasks
            .into_iter()
            .map(|task| {
                Rc::new(RefCell::new(TaskTreeBuilder {
                    task,
                    parent: None,
                    subtasks: vec![],
                }))
            })
            .partition(|task| task.borrow().task.parent_id.is_none());

        let mut tasks: HashMap<_, Rc<RefCell<TaskTreeBuilder>>> = top_level_tasks
            .into_iter()
            .map(|task| (task.borrow().task.id, task.clone()))
            .collect();

        let mut fails = 0; // Tracks for infinite loop on subtasks
        while !subtasks.is_empty() && fails <= subtasks.len() {
            let subtask = subtasks.pop_front().unwrap();
            let parent = tasks.entry(subtask.borrow().task.parent_id.unwrap());
            if let Entry::Vacant(_) = parent {
                fails += 1;
                subtasks.push_back(subtask);
                continue;
            }
            fails = 0;
            parent.and_modify(|entry| {
                subtask.borrow_mut().parent = Some(());
                entry.borrow_mut().subtasks.push(subtask.clone())
            });
            tasks.insert(subtask.borrow().task.id, subtask.clone());
        }

        if !subtasks.is_empty() {
            bail!("missing parent nodes in {} subtasks", subtasks.len(),);
        }
        Ok(tasks
            .into_iter()
            .filter(|(_, c)| c.borrow().parent.is_none())
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(_, c)| {
                Rc::try_unwrap(c)
                    .expect("only single reference")
                    .into_inner()
                    .finalize()
            })
            .collect())
    }
}

#[cfg(test)]
impl Task {
    /// This is initializer is used for tests, as in general the tool relies on the API and not
    /// local state.
    pub fn new(id: TaskID, content: &str) -> Task {
        Task {
            id,
            project_id: 0,
            section_id: None,
            content: content.to_string(),
            description: String::new(),
            completed: false,
            label_ids: Vec::new(),
            parent_id: None,
            order: 0,
            priority: Priority::default(),
            due: None,
            url: String::new(),
            comment_count: 0,
            assignee: None,
            assigner: None,
            created: chrono::Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_no_subtasks() {
        let tasks = vec![
            Task::new(1, "one"),
            Task::new(2, "two"),
            Task::new(3, "three"),
        ];
        let trees = TaskTree::from_tasks(tasks).unwrap();
        assert_eq!(trees.len(), 3);
    }

    #[test]
    fn test_tree_some_subtasks() {
        let tasks = vec![
            Task::new(1, "one"),
            Task::new(2, "two"),
            Task::new(3, "three"),
            Task {
                parent_id: Some(1),
                ..Task::new(4, "four")
            },
        ];
        let trees = TaskTree::from_tasks(tasks).unwrap();
        assert_eq!(trees.len(), 3);
        let task = trees.iter().filter(|t| t.task.id == 1).collect::<Vec<_>>();
        assert_eq!(task.len(), 1);
        let task = task[0];
        assert_eq!(task.subtasks.len(), 1);
        assert_eq!(task.subtasks[0].task.id, 4);
        for task in trees.into_iter().filter(|t| t.task.id != 1) {
            assert_eq!(task.subtasks.len(), 0);
        }
    }

    #[test]
    fn task_tree_complex_subtasks() {
        let tasks = vec![
            Task::new(1, "one"),
            Task {
                parent_id: Some(1),
                ..Task::new(2, "two")
            },
            Task {
                parent_id: Some(2),
                ..Task::new(3, "three")
            },
            Task {
                parent_id: Some(3),
                ..Task::new(4, "four")
            },
        ];
        let trees = TaskTree::from_tasks(tasks).unwrap();
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0].task.id, 1);
        assert_eq!(trees[0].subtasks[0].task.id, 2);
        assert_eq!(trees[0].subtasks[0].subtasks[0].task.id, 3);
        assert_eq!(trees[0].subtasks[0].subtasks[0].subtasks[0].task.id, 4);
    }

    #[test]
    fn task_tree_bad_input() {
        let tasks = vec![
            Task {
                parent_id: Some(1),
                ..Task::new(2, "two")
            },
            Task {
                parent_id: Some(2),
                ..Task::new(3, "three")
            },
        ];
        assert!(TaskTree::from_tasks(tasks).is_err());
    }
}